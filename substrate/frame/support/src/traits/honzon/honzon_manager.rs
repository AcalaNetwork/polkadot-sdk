//! This module provides the core traits for the Honzon protocol.

use sp_core::U256;
use sp_runtime::DispatchResult;
use sp_std::prelude::*;

use super::{Position, Ratio, ExchangeRate};

/// A trait for handling emergency shutdowns.
pub trait EmergencyShutdown {
	/// Returns `true` if the system is in shutdown mode.
	fn is_shutdown() -> bool;
}

/// A trait for managing the Honzon protocol, intended for use with EVM+.
pub trait HonzonManager<AccountId, BalanceAdjustment, Balance> {
	/// Adjusts a CDP loan.
	fn adjust_loan(
		who: &AccountId,
		collateral_adjustment: BalanceAdjustment,
		debit_adjustment: BalanceAdjustment,
	) -> DispatchResult;
	/// Closes a CDP loan using a DEX.
	fn close_loan_by_dex(who: AccountId, max_collateral_amount: Balance) -> DispatchResult;
	/// Returns the CDP for a given account.
	fn get_position(who: &AccountId) -> Position<Balance>;
	/// Returns the liquidation ratio for the collateral.
	fn get_collateral_parameters() -> Vec<U256>;
	/// Returns the current collateral-to-debit ratio of a CDP.
	fn get_current_collateral_ratio(who: &AccountId) -> Option<Ratio>;
	/// Returns the exchange rate of debit units to debit value.
	fn get_debit_exchange_rate() -> ExchangeRate;
}
