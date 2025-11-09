use crate::*;

impl<T: Config> Pallet<T> {
	// settle cdp has debit when emergency shutdown
	pub fn settle_cdp_has_debit(who: AccountIdOf<T>) -> DispatchResult {
		let currency_id = T::GetNativeCurrencyId::get();
		let Position { collateral, debit, stability_fee } = <LoansOf<T>>::positions(&who);
		let stability_fee = Rate::from_inner(stability_fee.into_inner());
		let stability_fee = Self::get_effective_stability_fee(stability_fee, &who);
		ensure!(!debit.is_zero(), Error::<T>::NoDebitValue);

		let settle_price: Price =
			T::PriceSource::get_relative_price(T::GetStableCurrencyId::get(), currency_id)
				.ok_or(Error::<T>::InvalidFeedPrice)?;
		let bad_debt_value = Self::convert_to_debit_value(debit, stability_fee);
		let confiscate_collateral_amount =
			sp_std::cmp::min(settle_price.saturating_mul_int(bad_debt_value), collateral);

		<LoansOf<T>>::confiscate_collateral_and_debit(&who, confiscate_collateral_amount, debit)?;

		Self::deposit_event(Event::SettleCDPInDebit { owner: who });
		Ok(())
	}

	// close cdp has debit by swap collateral to exact debit
	#[transactional]
	pub fn close_cdp_has_debit_by_dex(
		who: AccountIdOf<T>,
		max_collateral_amount: pallet_loans::BalanceOf<T>,
	) -> DispatchResult {
		let Position { collateral, debit, stability_fee } = <LoansOf<T>>::positions(&who);
		let stability_fee = Rate::from_inner(stability_fee.into_inner());
		ensure!(!debit.is_zero(), Error::<T>::NoDebitValue);
		ensure!(Self::is_cdp_safe(collateral, debit, stability_fee, &who)?, Error::<T>::MustBeSafe);
		let stability_fee = Self::get_effective_stability_fee(stability_fee, &who);

		<LoansOf<T>>::confiscate_collateral_and_debit(&who, collateral, debit)?;

		let debit_value = Self::convert_to_debit_value(debit, stability_fee);
		let collateral_supply = collateral.min(max_collateral_amount);

		let (actual_supply_collateral, _) = <T as Config>::CDPTreasury::swap_collateral_to_stable(
			SwapLimit::ExactTarget(collateral_supply, debit_value),
			false,
		)?;

		let refund_collateral_amount = collateral
			.checked_sub(&actual_supply_collateral)
			.expect("swap success means collateral >= actual_supply_collateral; qed");
		<T as Config>::CDPTreasury::withdraw_collateral(&who, refund_collateral_amount)?;

		Self::deposit_event(Event::CloseCDPInDebitByDEX {
			owner: who,
			sold_collateral_amount: actual_supply_collateral,
			refund_collateral_amount,
			debit_value,
		});
		Ok(())
	}

	/// Liquidate an unsafe CDP and return the weight consumed.
	pub fn liquidate_unsafe_cdp(who: AccountIdOf<T>) -> Result<Weight, DispatchError> {
		let Position { collateral, debit, stability_fee } = <LoansOf<T>>::positions(&who);

		ensure!(!Self::is_cdp_safe(collateral, debit, stability_fee)?, Error::<T>::MustBeUnsafe);

		<LoansOf<T>>::confiscate_collateral_and_debit(&who, collateral, debit, stability_fee)?;

		let bad_debt_value = Self::convert_to_debit_value(debit, stability_fee);
		let liquidation_penalty = Self::get_liquidation_penalty();
		let target_stable_amount = liquidation_penalty.saturating_mul_acc_int(bad_debt_value);

		Self::handle_liquidated_collateral(&who, collateral, target_stable_amount)?;

		Self::deposit_event(Event::LiquidateUnsafeCDP {
			owner: who,
			collateral_amount: collateral,
			bad_debt_value,
			target_amount: target_stable_amount,
		});
		Ok(T::WeightInfo::liquidate(1))
	}

	/// Handle liquidated collateral by priority
	pub fn handle_liquidated_collateral(
		who: &AccountIdOf<T>,
		amount: pallet_loans::BalanceOf<T>,
		target_stable_amount: pallet_loans::BalanceOf<T>,
	) -> DispatchResult {
		let currency_id = T::GetNativeCurrencyId::get();
		if target_stable_amount.is_zero() {
			if !amount.is_zero() {
				<T as Config>::CDPTreasury::withdraw_collateral(who, amount)?;
			}
			return Ok(());
		}
		LiquidateByPriority::<T>::liquidate(who, currency_id, amount, target_stable_amount)
	}
}
