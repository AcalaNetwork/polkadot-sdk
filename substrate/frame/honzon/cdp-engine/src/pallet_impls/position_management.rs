use crate::*;

impl<T: Config> Pallet<T> {
	pub fn adjust_position(
		who: &AccountIdOf<T>,
		collateral_adjustment: pallet_loans::BalanceAdjustment<pallet_loans::BalanceOf<T>>,
		debit_adjustment: pallet_loans::BalanceAdjustment<pallet_loans::BalanceOf<T>>,
		maybe_new_stability_fee: Option<Rate>,
	) -> DispatchResult {
		<LoansOf<T>>::adjust_position(
			who,
			collateral_adjustment,
			debit_adjustment,
			maybe_new_stability_fee,
		)?;
		Ok(())
	}

	pub fn adjust_position_by_debit_value(
		who: &AccountIdOf<T>,
		collateral_adjustment: pallet_loans::BalanceAdjustment<pallet_loans::BalanceOf<T>>,
		debit_value_adjustment: pallet_loans::BalanceAdjustment<pallet_loans::BalanceOf<T>>,
	) -> DispatchResult {
		let Position { debit, stability_fee, .. } = <LoansOf<T>>::positions(who);
		let position_stability_fee = Rate::from_inner(stability_fee.into_inner());
		let effective_stability_fee =
			Self::get_effective_stability_fee(position_stability_fee, who);

		match &debit_value_adjustment {
			pallet_loans::BalanceAdjustment::Decrease(amount) => {
				let debit_adjustment_abs =
					Self::convert_to_debit_balance(*amount, effective_stability_fee);
				let actual_adjustment_abs = debit.min(debit_adjustment_abs);
				let debit_adjustment =
					pallet_loans::BalanceAdjustment::decrease(actual_adjustment_abs);

				Self::adjust_position(who, collateral_adjustment, debit_adjustment, None)?;
			},
			pallet_loans::BalanceAdjustment::Increase(amount) => {
				let new_stability_fee = Self::interest_rate_per_sec();
				let debit_adjustment_abs =
					Self::convert_to_debit_balance(*amount, new_stability_fee);
				let debit_adjustment =
					pallet_loans::BalanceAdjustment::increase(debit_adjustment_abs);

				Self::adjust_position(
					who,
					collateral_adjustment,
					debit_adjustment,
					Some(new_stability_fee),
				)?;
			},
		}

		Ok(())
	}

	/// Generate new debit in advance, buy collateral and deposit it into CDP,
	/// and the collateral ratio will be reduced but CDP must still be at valid risk.
	#[transactional]
	pub fn expand_position_collateral(
		who: &AccountIdOf<T>,
		increase_debit_value: pallet_loans::BalanceOf<T>,
		min_increase_collateral: pallet_loans::BalanceOf<T>,
	) -> DispatchResult {
		let currency_id = T::GetNativeCurrencyId::get();
		let loans_module_account = <LoansOf<T>>::account_id();

		<T as Config>::CDPTreasury::issue_debit(&loans_module_account, increase_debit_value, true)?;

		let increase_collateral = T::Swap::swap_exact_tokens_for_tokens(
			loans_module_account.clone(),
			vec![T::GetStableCurrencyId::get().into(), currency_id.into()],
			increase_debit_value,
			Some(min_increase_collateral),
			loans_module_account.clone(),
			true,
		)?;

		let collateral_adjustment = pallet_loans::BalanceAdjustment::increase(increase_collateral);
		let new_stability_fee = Self::interest_rate_per_sec();
		let increase_debit_balance =
			Self::convert_to_debit_balance(increase_debit_value, new_stability_fee);
		let debit_adjustment = pallet_loans::BalanceAdjustment::increase(increase_debit_balance);
		Self::adjust_position(
			who,
			collateral_adjustment,
			debit_adjustment,
			Some(new_stability_fee),
		)?;

		let Position { collateral, debit, .. } = <LoansOf<T>>::positions(who);
		Self::do_check_position(collateral, debit, false)?;
		let Position { debit: total_debit, .. } = <LoansOf<T>>::total_positions();
		Self::do_check_debit_cap(total_debit)?;
		Ok(())
	}

	/// Sell the collateral locked in CDP to get stable coin to repay the debit,
	/// and the collateral ratio will be increased.
	#[transactional]
	fn shrink_position_debit(
		who: &AccountIdOf<T>,
		decrease_collateral: pallet_loans::BalanceOf<T>,
		min_decrease_debit_value: pallet_loans::BalanceOf<T>,
	) -> DispatchResult {
		let currency_id = T::GetNativeCurrencyId::get();
		let loans_module_account = <LoansOf<T>>::account_id();
		let stable_currency_id = T::GetStableCurrencyId::get();
		let Position { collateral, debit, stability_fee } = <LoansOf<T>>::positions(who);
		let position_stability_fee = Rate::from_inner(stability_fee.into_inner());
		let effective_stability_fee =
			Self::get_effective_stability_fee(position_stability_fee, who);

		ensure!(decrease_collateral <= collateral, Error::<T>::CollateralNotEnough);

		let actual_stable_amount = T::Swap::swap_exact_tokens_for_tokens(
			loans_module_account.clone(),
			vec![currency_id.into(), stable_currency_id.into()],
			decrease_collateral,
			Some(min_decrease_debit_value),
			loans_module_account.clone(),
			true,
		)?;

		let collateral_adjustment = pallet_loans::BalanceAdjustment::decrease(decrease_collateral);
		let previous_debit_value = Self::convert_to_debit_value(debit, effective_stability_fee);
		let (decrease_debit_value, decrease_debit_balance) =
			if actual_stable_amount >= previous_debit_value {
				<T as Config>::Tokens::transfer(
					stable_currency_id,
					&loans_module_account,
					who,
					actual_stable_amount.saturating_sub(previous_debit_value),
					Preservation::Protect,
				)?;

				(previous_debit_value, debit)
			} else {
				(
					actual_stable_amount,
					Self::convert_to_debit_balance(actual_stable_amount, effective_stability_fee),
				)
			};

		let debit_adjustment = pallet_loans::BalanceAdjustment::decrease(decrease_debit_balance);
		Self::adjust_position(who, collateral_adjustment, debit_adjustment, None)?;

		<T as Config>::CDPTreasury::burn_debit(&loans_module_account, decrease_debit_value)?;

		let Position { collateral: updated_collateral, debit: updated_debit, .. } =
			<LoansOf<T>>::positions(who);
		Self::do_check_position(updated_collateral, updated_debit, false)?;
		Ok(())
	}
}
