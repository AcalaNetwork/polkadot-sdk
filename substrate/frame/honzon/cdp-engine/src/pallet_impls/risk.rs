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

	// --- Shared validation helpers for risk management (trait + internal) ---
	/// Shared validator for position collateral/debt, reused by both trait and pallet logic.
	pub fn do_check_position(
		collateral_balance: pallet_loans::BalanceOf<T>,
		debit_units: pallet_loans::DebitUnitOf<T>,
		debit_interest_rate: Rate,
		check_required_ratio: bool,
	) -> Result<(), Error<T>> {
		if !debit_units.is_zero() {
			let feed_price = <T as Config>::PriceSource::get_relative_price(
				T::GetNativeCurrencyId::get(),
				T::GetStableCurrencyId::get(),
			)
			.ok_or(Error::<T>::InvalidFeedPrice)?;
			let debit_value = Self::do_debit_units_to_value(debit_units, debit_interest_rate);
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

			ensure!(
				collateral_ratio >= Self::get_liquidation_ratio(),
				Error::<T>::BelowLiquidationRatio
			);

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
		for (stability_fee, debit_units) in pallet_loans::TotalDebitByStabilityFee::<T>::iter() {
			if debit_units.is_zero() {
				continue;
			}
			let debit_value = Self::do_debit_units_to_value(debit_units, stability_fee);
			total_debit_value = total_debit_value.saturating_add(debit_value);
		}
		ensure!(total_debit_value <= hard_cap, Error::<T>::ExceedDebitValueHardCap);
		Ok(())
	}
}
