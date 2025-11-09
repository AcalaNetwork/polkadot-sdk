use crate::*;

impl<T: Config> Pallet<T> {
	// accumulate interest for all debit active stability fee classes with outstanding debt
	pub fn accumulate_interest(now_secs: u64, last_accumulation_secs: u64) -> u32 {
		if !T::EmergencyShutdown::is_shutdown() && !now_secs.is_zero() {
			let interval_secs = now_secs.saturating_sub(last_accumulation_secs);
			let mut touched_entries: u32 = 0;

			for (stability_fee, total_debits) in pallet_loans::TotalDebitByStabilityFee::<T>::iter()
			{
				if total_debits.is_zero() {
					continue;
				}

				let rate_to_accumulate = Self::compound_interest_rate(stability_fee, interval_secs);
				if rate_to_accumulate.is_zero() {
					continue;
				}

				let compound_factor = Self::get_or_init_compound_factor(stability_fee);
				let compound_factor_increment = compound_factor.saturating_mul(rate_to_accumulate);
				let stable_coin_balance_to_issue =
					compound_factor_increment.saturating_mul_int(total_debits);

				let res =
					<T as Config>::CDPTreasury::on_system_surplus(stable_coin_balance_to_issue);
				match res {
					Ok(_) => {
						let new_compound_factor =
							compound_factor.saturating_add(compound_factor_increment);
						CompoundFactorStorage::<T>::insert(stability_fee, new_compound_factor);
						touched_entries = touched_entries.saturating_add(1);
					},
					Err(e) => {
						log::warn!(
							target: "cdp-engine",
							"on_system_surplus: failed to on system surplus {:?}: {:?}. This is unexpected but should be safe",
							stable_coin_balance_to_issue,
							e
						);
					},
				}
			}

			LastAccumulationSecs::<T>::put(now_secs);
			return touched_entries.saturating_add(1);
		}

		LastAccumulationSecs::<T>::put(now_secs);
		0
	}

	/// Calculates the compounded interest rate for a given rate per second over a specified number
	/// of seconds.
	///
	/// The formula is:
	/// `(1 + rate_per_sec).pow(secs) - 1`
	pub fn compound_interest_rate(rate_per_sec: Rate, secs: u64) -> Rate {
		rate_per_sec
			.saturating_add(Rate::one())
			.saturating_pow(secs.unique_saturated_into())
			.saturating_sub(Rate::one())
	}

	/// Gets the compound factor for the given stability fee, initializing it with the default
	/// if not yet set.
	pub fn get_or_init_compound_factor(stability_fee: Rate) -> CompoundFactor {
		if let Some(compound_factor) = CompoundFactorStorage::<T>::get(stability_fee) {
			compound_factor
		} else {
			let default_rate = T::DefaultCompoundFactor::get();
			CompoundFactorStorage::<T>::insert(stability_fee, default_rate);
			default_rate
		}
	}

	/// Converts a given debit balance to its value based on the stability fee's compound factor.
	pub fn convert_to_debit_value(
		debit_balance: pallet_loans::BalanceOf<T>,
		stability_fee: Rate,
	) -> pallet_loans::BalanceOf<T> {
		Self::get_or_init_compound_factor(stability_fee).saturating_mul_int(debit_balance)
	}

	/// Converts a debit value amount into a debit balance using the given stability fee's compound
	/// factor
	pub fn convert_to_debit_balance(
		debit_value: pallet_loans::BalanceOf<T>,
		stability_fee: Rate,
	) -> pallet_loans::BalanceOf<T> {
		Self::get_or_init_compound_factor(stability_fee)
			.reciprocal()
			.saturating_mul_int(debit_value)
	}
}
