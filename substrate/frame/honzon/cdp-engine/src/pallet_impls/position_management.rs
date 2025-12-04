use crate::*;

impl<T: Config> Pallet<T> {
	pub fn adjust_position(
		who: &AccountIdOf<T>,
		collateral_adjustment: pallet_loans::BalanceAdjustment<pallet_loans::BalanceOf<T>>,
		debit_value_adjustment: pallet_loans::BalanceAdjustment<pallet_loans::BalanceOf<T>>,
	) -> DispatchResult {
		let maybe_new_stability_fee = if debit_value_adjustment.is_increase() {
			Some(Self::interest_rate_per_sec())
		} else {
			None
		};
		<LoansOf<T>>::adjust_position(
			who,
			collateral_adjustment,
			debit_value_adjustment,
			maybe_new_stability_fee,
		)?;
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
		let loans_module_account = <LoansOf<T>>::account_id();

		<T as Config>::CDPTreasury::issue_debit(&loans_module_account, increase_debit_value, true)?;

		let increase_collateral = T::Swap::swap_exact_tokens_for_tokens(
			loans_module_account.clone(),
			vec![T::GetStableCurrencyId::get().into(), T::GetNativeCurrencyId::get().into()],
			increase_debit_value,
			Some(min_increase_collateral),
			loans_module_account.clone(),
			true,
		)?;

		<LoansOf<T>>::adjust_position(
			who,
			pallet_loans::BalanceAdjustment::increase(increase_collateral),
			pallet_loans::BalanceAdjustment::increase(increase_debit_value),
			None,
		)?;

		let Position { collateral, debit, .. } = <LoansOf<T>>::positions(who);
		Self::do_check_position(collateral, debit.units, debit.stability_fee, false)?;
		Self::do_check_debit_cap()?;
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
		let loans_module_account = <LoansOf<T>>::account_id();
		let Position { collateral, debit } = <LoansOf<T>>::positions(who);
		// TODO: check how to handle the effective stability fee.
		let effective_stability_fee = debit.stability_fee;
		ensure!(decrease_collateral <= collateral, Error::<T>::CollateralNotEnough);

		let actual_stable_amount = T::Swap::swap_exact_tokens_for_tokens(
			loans_module_account.clone(),
			vec![T::GetNativeCurrencyId::get().into(), T::GetStableCurrencyId::get().into()],
			decrease_collateral,
			Some(min_decrease_debit_value),
			loans_module_account.clone(),
			true,
		)?;

		let previous_debit_value =
			Self::do_debit_units_to_value(debit.units, effective_stability_fee);
		let decrease_debit_value = if actual_stable_amount >= previous_debit_value {
			<T as Config>::Tokens::transfer(
				T::GetStableCurrencyId::get(),
				&loans_module_account,
				who,
				actual_stable_amount.saturating_sub(previous_debit_value),
				Preservation::Protect,
			)?;
			previous_debit_value
		} else {
			actual_stable_amount
		};

		<LoansOf<T>>::adjust_position(
			who,
			pallet_loans::BalanceAdjustment::decrease(decrease_collateral),
			pallet_loans::BalanceAdjustment::decrease(decrease_debit_value),
			None,
		)?;

		<T as Config>::CDPTreasury::burn_debit(&loans_module_account, decrease_debit_value)?;

		let Position { collateral, debit, .. } = <LoansOf<T>>::positions(who);
		Self::do_check_position(collateral, debit.units, debit.stability_fee, false)?;
		Ok(())
	}
}
