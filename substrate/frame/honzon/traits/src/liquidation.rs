#![cfg_attr(not(feature = "std"), no_std)]

use sp_runtime::DispatchError;

/// A trait for something that can participate in a liquidation.
pub trait LiquidationTarget<AccountId, CurrencyId, Balance> {
	/// Attempt to liquidate collateral.
	///
	/// The `collateral_to_sell` is offered for sale to cover `debit_to_cover`.
	/// The implementer can buy some or all of it.
	///
	/// - `who`: The account holding the collateral to be liquidated.
	/// - `collateral_currency`: The currency of the collateral being sold.
	/// - `collateral_to_sell`: The amount of collateral on offer.
	/// - `debit_to_cover`: The amount of debit that needs to be covered.
	///
	/// Returns `Ok((liquidated_collateral, covered_debit))`
	fn liquidate(
		who: &AccountId,
		collateral_currency: CurrencyId,
		collateral_to_sell: Balance,
		debit_to_cover: Balance,
	) -> Result<(Balance, Balance), DispatchError>;
}

/// A simple liquidation strategy for use in tests and mocks.
///
/// The strategy does not perform any actual liquidation, instead it returns
/// zeroed values so the caller can treat the entire collateral and debit as
/// leftovers.
#[derive(Default, Clone, Copy, Eq, PartialEq, Debug)]
pub struct MockLiquidationStrategy;

impl<AccountId, CurrencyId, Balance: Default>
	LiquidationTarget<AccountId, CurrencyId, Balance> for MockLiquidationStrategy
{
	fn liquidate(
		_who: &AccountId,
		_collateral_currency: CurrencyId,
		_collateral_to_sell: Balance,
		_debit_to_cover: Balance,
	) -> Result<(Balance, Balance), DispatchError> {
		Ok((Balance::default(), Balance::default()))
	}
}
