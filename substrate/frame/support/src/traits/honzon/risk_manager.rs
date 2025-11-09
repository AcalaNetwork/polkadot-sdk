use sp_runtime::DispatchResult;
/// A trait for managing the risk of the protocol.
pub trait RiskManager<Unit, Balance, Rate> {
	/// Returns the value of a given amount of debit.
	fn debit_units_to_value(interest_rate: Rate, debit_units: Unit) -> Balance;

	/// Return the debit units of a given amount of value.
	fn value_to_debit_units(interest_rate: Rate, debit_value: Balance) -> Unit;

	/// Checks if a position is valid.
	fn check_position_valid(
		collateral_balance: Balance,
		debit_units: Unit,
		debit_interest_rate: Rate,
		check_required_ratio: bool,
	) -> DispatchResult;

	/// Checks if the total debit has reached its cap.
	fn check_debit_cap() -> DispatchResult;
}

impl<Unit: Default, Balance: Default, Rate: Default> RiskManager<Unit, Balance, Rate> for () {
	fn debit_units_to_value(_interest_rate: Rate, _debit_units: Unit) -> Balance {
		Default::default()
	}

	fn value_to_debit_units(_interest_rate: Rate, _debit_value: Balance) -> Unit {
		Default::default()
	}

	fn check_position_valid(
		_collateral_balance: Balance,
		_debit_units: Unit,
		_debit_interest_rate: Rate,
		_check_required_ratio: bool,
	) -> DispatchResult {
		Ok(())
	}

	fn check_debit_cap() -> DispatchResult {
		Ok(())
	}
}
