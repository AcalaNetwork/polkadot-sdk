use crate::*;

impl<T: Config> Pallet<T> {
	// settle cdp has debit when emergency shutdown
	pub fn do_settle(who: AccountIdOf<T>) -> DispatchResult {
		let Position { collateral, debit } = <LoansOf<T>>::positions(&who);

		ensure!(!debit.units.is_zero(), Error::<T>::NoDebitValue);

		let settle_price: Price = T::PriceSource::get_relative_price(
			T::GetStableCurrencyId::get(),
			T::GetNativeCurrencyId::get(),
		)
		.ok_or(Error::<T>::InvalidFeedPrice)?;
		let bad_debt_value = Self::do_debit_units_to_value(debit.units, debit.stability_fee);
		let confiscate_collateral_amount =
			sp_std::cmp::min(settle_price.saturating_mul_int(bad_debt_value), collateral);

		<LoansOf<T>>::confiscate_collateral_and_debit(
			&who,
			confiscate_collateral_amount,
			bad_debt_value,
		)?;

		Self::deposit_event(Event::SettleCDPInDebit { owner: who });
		Ok(())
	}

	// close cdp has debit by swap collateral to exact debit
	#[transactional]
	pub fn close_cdp_has_debit_by_dex(
		who: AccountIdOf<T>,
		max_collateral_amount: pallet_loans::BalanceOf<T>,
	) -> DispatchResult {
		let Position { collateral, debit } = <LoansOf<T>>::positions(&who);
		// ensure the CDP has debit.
		ensure!(!debit.units.is_zero(), Error::<T>::NoDebitValue);
		// ensure the CDP is safe.
		ensure!(
			Self::is_cdp_safe(collateral, debit.units, debit.stability_fee)?,
			Error::<T>::MustBeSafe
		);

		// liquidate the collateral and debit to the CDP treasury.
		let debit_value = Self::do_debit_units_to_value(debit.units, debit.stability_fee);
		<LoansOf<T>>::confiscate_collateral_and_debit(&who, collateral, debit_value)?;

		let collateral_supply = collateral.min(max_collateral_amount);
		// try to swap collateral to stable.
		let (actual_supply_collateral, _) = <T as Config>::CDPTreasury::swap_collateral_to_stable(
			SwapLimit::ExactTarget(collateral_supply, debit_value),
			false,
		)?;

		// refund the remaining collateral to the CDP owner.
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
	pub fn do_liquidate(who: AccountIdOf<T>) -> Result<Weight, DispatchError> {
		let Position { collateral, debit } = <LoansOf<T>>::positions(&who);

		Self::do_check_position(collateral, debit.units, debit.stability_fee, false)?;

		let bad_debt_value = Self::do_debit_units_to_value(debit.units, debit.stability_fee);

		<LoansOf<T>>::confiscate_collateral_and_debit(&who, collateral, bad_debt_value)?;

		let liquidation_penalty = Self::liquidation_penalty();
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
	fn handle_liquidated_collateral(
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
