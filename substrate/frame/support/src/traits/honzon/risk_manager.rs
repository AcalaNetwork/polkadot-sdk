use sp_runtime::DispatchResult;
/// A trait for managing the risk of the protocol.
pub trait RiskManager<AccountId, CurrencyId, Balance, DebitBalance> {
	/// Returns the value of a given amount of debit.
	fn get_debit_value(currency_id: CurrencyId, debit_balance: DebitBalance) -> Balance;

	/// Checks if a position is valid.
	fn check_position_valid(
		currency_id: CurrencyId,
		collateral_balance: Balance,
		debit_balance: DebitBalance,
		check_required_ratio: bool,
	) -> DispatchResult;

	/// Checks if the total debit for a currency has reached its cap.
	fn check_debit_cap(
		currency_id: CurrencyId,
		total_debit_balance: DebitBalance,
	) -> DispatchResult;
}

#[cfg(feature = "std")]
impl<AccountId, CurrencyId, Balance: Default, DebitBalance>
	RiskManager<AccountId, CurrencyId, Balance, DebitBalance> for ()
{
	fn get_debit_value(_currency_id: CurrencyId, _debit_balance: DebitBalance) -> Balance {
		Default::default()
	}

	fn check_position_valid(
		_currency_id: CurrencyId,
		_collateral_balance: Balance,
		_debit_balance: DebitBalance,
		_check_required_ratio: bool,
	) -> DispatchResult {
		Ok(())
	}

	fn check_debit_cap(
		_currency_id: CurrencyId,
		_total_debit_balance: DebitBalance,
	) -> DispatchResult {
		Ok(())
	}
}
