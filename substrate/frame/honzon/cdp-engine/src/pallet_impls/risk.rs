use crate::*;

impl<T: Config> Pallet<T> {
	/// Get the effective stability fee for a given account.
	pub fn get_effective_stability_fee(stability_fee: Rate, who: &AccountIdOf<T>) -> Rate {
		let current_stability_fee = Self::interest_rate_per_sec();
		let mut effective_fee = sp_std::cmp::min(stability_fee, current_stability_fee);

		if let Some(vault_id) = VaultIdByAccountId::<T>::get(who) {
			if let Some(override_fee) = Self::stability_fee_overrides(vault_id) {
				effective_fee = sp_std::cmp::min(effective_fee, override_fee);
			}
		}

		effective_fee
	}

	fn get_debit_value(
		stability_fee: Rate,
		debit_balance: pallet_loans::BalanceOf<T>,
	) -> pallet_loans::BalanceOf<T> {
		Self::get_or_init_compound_factor(stability_fee).saturating_mul_int(debit_balance)
	}

	/// Calculates the collateral ratio given collateral, debit, price, and compound factor.
	pub fn calculate_collateral_ratio(
		collateral_balance: pallet_loans::BalanceOf<T>,
		collateral_price: Price,
		debit_value: pallet_loans::BalanceOf<T>,
	) -> Ratio {
		let locked_collateral_value = collateral_price.saturating_mul_int(collateral_balance);

		// if debit_value is zero, return max value
		Ratio::checked_from_rational(locked_collateral_value, debit_value)
			.unwrap_or_else(Ratio::max_value)
	}

	/// Check if collateral ratio below liquidation ratio
	pub fn is_cdp_safe(
		collateral_balance: pallet_loans::BalanceOf<T>,
		debit_balance: pallet_loans::BalanceOf<T>,
		stability_fee: Rate,
	) -> Result<bool, Error<T>> {
		let feed_price = T::PriceSource::get_relative_price(
			T::GetNativeCurrencyId::get(),
			T::GetStableCurrencyId::get(),
		)
		.ok_or(Error::<T>::InvalidFeedPrice)?;
		let debit_value = Self::get_debit_value(stability_fee, debit_balance);
		let collateral_ratio =
			Self::calculate_collateral_ratio(collateral_balance, debit_value, feed_price);
		Ok(collateral_ratio >= Self::get_liquidation_ratio())
	}

	// --- Shared validation helpers for risk management (trait + internal) ---
	/// Shared validator for position collateral/debt, reused by both trait and pallet logic.
	pub fn do_check_position(
		collateral_balance: pallet_loans::BalanceOf<T>,
		debit_balance: pallet_loans::BalanceOf<T>,
		stability_fee: Rate,
		check_required_ratio: bool,
	) -> Result<(), Error<T>> {
		if !debit_balance.is_zero() {
			let feed_price = <T as Config>::PriceSource::get_relative_price(
				T::GetNativeCurrencyId::get(),
				T::GetStableCurrencyId::get(),
			)
			.ok_or(Error::<T>::InvalidFeedPrice)?;
			let debit_value = Self::get_debit_value(stability_fee, debit_balance);
			let collateral_ratio =
				Self::calculate_collateral_ratio(collateral_balance, feed_price, debit_value);

			if check_required_ratio {
				if let Some(required_collateral_ratio) = Self::required_collateral_ratio() {
					ensure!(
						collateral_ratio >= required_collateral_ratio,
						Error::<T>::BelowRequiredCollateralRatio
					);
				}
			}

			let liquidation_ratio = Self::get_liquidation_ratio();
			ensure!(collateral_ratio >= liquidation_ratio, Error::<T>::BelowLiquidationRatio);

			ensure!(
				debit_value >= T::MinimumDebitValue::get(),
				Error::<T>::RemainDebitValueTooSmall,
			);
		} else if !collateral_balance.is_zero() {
			ensure!(
				collateral_balance >= T::MinimumCollateralAmount::get(),
				Error::<T>::CollateralAmountBelowMinimum,
			);
		}

		Ok(())
	}

	/// Shared checker for cap on aggregate debits.
	pub fn do_check_debit_cap() -> Result<(), Error<T>> {
		let hard_cap = Self::maximum_total_debit_value();
		let mut total_debit_value = pallet_loans::BalanceOf::<T>::zero();
		for (stability_fee, debit_balance) in pallet_loans::TotalDebitByStabilityFee::<T>::iter() {
			if debit_balance.is_zero() {
				continue;
			}
			let debit_value = Self::get_debit_value(stability_fee, debit_balance);
			total_debit_value = total_debit_value.saturating_add(debit_value);
		}
		ensure!(total_debit_value <= hard_cap, Error::<T>::ExceedDebitValueHardCap);
		Ok(())
	}
}
