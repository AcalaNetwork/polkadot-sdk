use sp_runtime::{DispatchError, DispatchResult};

use crate::traits::honzon::types::{Ratio, SwapLimit};

/// A trait for managing the Collateralized Debt Position (CDP) treasury.
pub trait CDPTreasury<AccountId> {
	/// Returns the account ID of the treasury.
	fn account_id() -> AccountId;
	/// The type of balance used in the treasury.
	type Balance;
	/// The type of currency used in the treasury.
	type CurrencyId;

	/// Returns the amount of surplus in the treasury.
	fn get_surplus_pool() -> Self::Balance;

	/// Returns the amount of debit in the treasury.
	fn get_debit_pool() -> Self::Balance;

	/// Returns the total amount of collateral in the treasury.
	fn get_total_collaterals() -> Self::Balance;

	/// Calculates the proportion of a specific debit amount relative to the whole system.
	fn get_debit_proportion(amount: Self::Balance) -> Ratio;

	/// Handles a system-wide debit event.
	fn on_system_debit(amount: Self::Balance) -> DispatchResult;

	/// Handles a system-wide surplus event.
	fn on_system_surplus(amount: Self::Balance) -> DispatchResult;

	/// Issues debit to a specified account.
	///
	/// If `backed` is true, the debit is backed by some assets; otherwise, the system
	/// debit will be increased by the same amount.
	fn issue_debit(who: &AccountId, debit: Self::Balance, backed: bool) -> DispatchResult;

	/// Burns debit from a specified account.
	fn burn_debit(who: &AccountId, debit: Self::Balance) -> DispatchResult;

	/// Deposits surplus from a specified account into the treasury.
	fn deposit_surplus(from: &AccountId, surplus: Self::Balance) -> DispatchResult;

	/// Withdraws surplus from the treasury to a specified account.
	fn withdraw_surplus(to: &AccountId, surplus: Self::Balance) -> DispatchResult;

	/// Deposits collateral from a specified account into the treasury.
	fn deposit_collateral(from: &AccountId, amount: Self::Balance) -> DispatchResult;

	/// Withdraws collateral from the treasury to a specified account.
	fn withdraw_collateral(to: &AccountId, amount: Self::Balance) -> DispatchResult;

	/// Pays surplus from the treasury.
	fn pay_surplus(amount: Self::Balance) -> DispatchResult;

	/// Refunds surplus to the treasury.
	fn refund_surplus(amount: Self::Balance) -> DispatchResult;
}

/// An extended `CDPTreasury` trait.
pub trait CDPTreasuryExtended<AccountId>: CDPTreasury<AccountId> {
	/// Swaps collateral to the stable currency.
	fn swap_collateral_to_stable(
		limit: SwapLimit<Self::Balance>,
		collateral_in_auction: bool,
	) -> sp_std::result::Result<(Self::Balance, Self::Balance), DispatchError>;

	/// Creates collateral auctions.
	fn create_collateral_auctions(
		amount: Self::Balance,
		target: Self::Balance,
		refund_receiver: AccountId,
		split: bool,
	) -> sp_std::result::Result<u32, DispatchError>;

	/// Returns the maximum number of auctions that can be created.
	fn max_auction() -> u32;
}
