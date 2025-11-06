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
		debit_balance: pallet_loans::BalanceOf<T>,
		price: Price,
		compound_factor: CompoundFactor,
	) -> Ratio {
		let locked_collateral_value = price.saturating_mul_int(collateral_balance);
		let debit_value = compound_factor.saturating_mul_int(debit_balance);

		// if debit_value is zero, return max value
		Ratio::checked_from_rational(locked_collateral_value, debit_value)
			.unwrap_or_else(Ratio::max_value)
	}

	pub fn is_cdp_safe(
		collateral_amount: pallet_loans::BalanceOf<T>,
		debit_amount: pallet_loans::BalanceOf<T>,
		stability_fee: Rate,
		who: &AccountIdOf<T>,
	) -> Result<bool, Error<T>> {
		let currency_id = T::GetNativeCurrencyId::get();
		let stable_currency_id = T::GetStableCurrencyId::get();
		let feed_price = T::PriceSource::get_relative_price(currency_id, stable_currency_id)
			.ok_or(Error::<T>::InvalidFeedPrice)?;
		let stability_fee = Self::get_effective_stability_fee(stability_fee, who);
		let compound_factor = Self::get_compound_factor(stability_fee);
		let collateral_ratio = Self::calculate_collateral_ratio(
			collateral_amount,
			debit_amount,
			feed_price,
			compound_factor,
		);
		if collateral_ratio < Self::get_liquidation_ratio() {
			Ok(false)
		} else {
			Ok(true)
		}
	}

	// --- Shared validation helpers for risk management (trait + internal) ---
	/// Shared validator for position collateral/debt, reused by both trait and pallet logic.
	pub fn do_check_position(
		collateral_balance: pallet_loans::BalanceOf<T>,
		debit_balance: pallet_loans::BalanceOf<T>,
		check_required_ratio: bool,
	) -> Result<(), Error<T>> {
		if !debit_balance.is_zero() {
			let compound_factor = Self::average_compound_factor();
			let debit_value = compound_factor.saturating_mul_int(debit_balance);
			let feed_price = <T as Config>::PriceSource::get_relative_price(
				T::GetNativeCurrencyId::get(),
				T::GetStableCurrencyId::get(),
			)
			.ok_or(Error::<T>::InvalidFeedPrice)?;
			let collateral_ratio = Self::calculate_collateral_ratio(
				collateral_balance,
				debit_balance,
				feed_price,
				compound_factor,
			);

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
	pub fn do_check_debit_cap(
		total_debit_balance: pallet_loans::BalanceOf<T>,
	) -> Result<(), Error<T>> {
		let hard_cap = Self::maximum_total_debit_value();
		let mut total_debit_value = pallet_loans::BalanceOf::<T>::zero();
		for (stability_fee, debit_balance) in pallet_loans::TotalDebitByStabilityFee::<T>::iter() {
			if debit_balance.is_zero() {
				continue;
			}
			let compound_factor = Self::get_compound_factor(stability_fee);
			total_debit_value =
				total_debit_value.saturating_add(compound_factor.saturating_mul_int(debit_balance));
		}
		if total_debit_value.is_zero() && !total_debit_balance.is_zero() {
			let compound_factor = Self::average_compound_factor();
			total_debit_value = compound_factor.saturating_mul_int(total_debit_balance);
		}
		ensure!(total_debit_value <= hard_cap, Error::<T>::ExceedDebitValueHardCap);
		Ok(())
	}
}
