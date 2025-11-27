use sp_runtime::DispatchResult;
/// A trait for managing the risk of the protocol.
pub trait RiskManager<Unit, Balance, Rate> {
	/// Returns the value of a given amount of debit.
	fn debit_units_to_value(debit_units: Unit, stability_fee: Rate) -> Balance;

	/// Return the debit units of a given amount of value.
	fn value_to_debit_units(debit_value: Balance, stability_fee: Rate) -> Unit;

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

use super::DebitUnit;
impl<Balance: Default, Rate: Default> RiskManager<DebitUnit<Balance>, Balance, Rate> for () {
	fn debit_units_to_value(debit_units: DebitUnit<Balance>, _stability_fee: Rate) -> Balance {
		debit_units.into_inner()
	}

	fn value_to_debit_units(debit_value: Balance, _stability_fee: Rate) -> DebitUnit<Balance> {
		DebitUnit::new(debit_value)
	}

	fn check_position_valid(
		_collateral_balance: Balance,
		_debit_units: DebitUnit<Balance>,
		_debit_interest_rate: Rate,
		_check_required_ratio: bool,
	) -> DispatchResult {
		Ok(())
	}

	fn check_debit_cap() -> DispatchResult {
		Ok(())
	}
}
