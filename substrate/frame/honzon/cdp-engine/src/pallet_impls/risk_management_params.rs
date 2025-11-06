use crate::*;

impl<T: Config> Pallet<T> {
	pub fn risk_management_params() -> RiskManagementParams<pallet_loans::BalanceOf<T>> {
		T::RiskManagementParams::get()
	}

	/// Returns the maximum total debit value that can be issued for the current collateral type.
	pub fn maximum_total_debit_value() -> pallet_loans::BalanceOf<T> {
		let params = Self::risk_management_params();
		params.maximum_total_debit_value
	}

	/// Returns the required collateral ratio, if set, for the current collateral type.
	pub fn required_collateral_ratio() -> Option<Ratio> {
		let params = Self::risk_management_params();
		params.required_collateral_ratio
	}

	/// Returns the interest rate per second for the current collateral type.
	pub fn interest_rate_per_sec() -> Rate {
		let params = Self::risk_management_params();
		params.interest_rate_per_sec
	}

	/// Returns the liquidation ratio for the current collateral type.
	pub fn get_liquidation_ratio() -> Ratio {
		let params = Self::risk_management_params();
		params.liquidation_ratio
	}

	/// Returns the current liquidation penalty rate for the collateral type, or the default if not
	/// set.
	pub fn get_liquidation_penalty() -> Rate {
		let params = Self::risk_management_params();
		params.liquidation_penalty
	}
}
