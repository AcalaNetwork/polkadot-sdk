use crate::{AccountIdOf, BalanceOf, Config, CurrencyIdOf};
use frame_support::{
	pallet_prelude::*,
	traits::honzon::{
		CDPTreasury, CDPTreasuryExtended, FractionalRate, LiquidateCollateral, PriceProvider,
		Ratio, SwapLimit,
	},
};
use sp_runtime::{
	traits::{Bounded, Saturating},
	FixedPointNumber,
};
/// Risk management param
#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq, Default, TypeInfo, MaxEncodedLen)]
pub struct RiskManagementParams<Balance> {
	/// Maximum total debit value generated from it, when reach the hard
	/// cap, CDP's owner cannot issue more stablecoin under the collateral
	/// type.
	pub maximum_total_debit_value: Balance,

	/// Extra interest rate per sec, `None` value means not set
	pub interest_rate_per_sec: Option<FractionalRate>,

	/// Liquidation ratio, when the collateral ratio of
	/// CDP under this collateral type is below the liquidation ratio, this
	/// CDP is unsafe and can be liquidated. `None` value means not set
	pub liquidation_ratio: Option<Ratio>,

	/// Liquidation penalty rate, when liquidation occurs,
	/// CDP will be deducted an additional penalty base on the product of
	/// penalty rate and debit value. `None` value means not set
	pub liquidation_penalty: Option<FractionalRate>,

	/// Required collateral ratio, if it's set, cannot adjust the position
	/// of CDP so that the current collateral ratio is lower than the
	/// required collateral ratio. `None` value means not set
	pub required_collateral_ratio: Option<Ratio>,
}

pub type LiquidateByPriority<T> = (LiquidateViaDex<T>, LiquidateViaAuction<T>);

pub struct LiquidateViaDex<T>(PhantomData<T>);
impl<T: Config> LiquidateCollateral<AccountIdOf<T>, CurrencyIdOf<T>, BalanceOf<T>>
	for LiquidateViaDex<T>
{
	fn liquidate(
		who: &AccountIdOf<T>,
		_collateral_currency_id: CurrencyIdOf<T>,
		amount: BalanceOf<T>,
		target_stable_amount: BalanceOf<T>,
	) -> DispatchResult {
		let currency_id = T::GetNativeCurrencyId::get();
		// calculate the supply limit by slippage limit for the price of oracle,
		let max_supply_limit = Ratio::one()
			.saturating_sub(T::MaxSwapSlippageCompareToOracle::get())
			.reciprocal()
			.unwrap_or_else(Ratio::max_value)
			.saturating_mul_int(
				T::PriceSource::get_relative_price(T::GetStableCurrencyId::get(), currency_id)
					.expect("the oracle price should be available because liquidation are triggered by it.")
					.saturating_mul_int(target_stable_amount),
			);
		let collateral_supply = amount.min(max_supply_limit);

		let (actual_supply_collateral, actual_target_amount) =
			<T as Config>::CDPTreasury::swap_collateral_to_stable(
				SwapLimit::ExactTarget(collateral_supply, target_stable_amount),
				false,
			)?;

		let refund_collateral_amount = amount
			.checked_sub(&actual_supply_collateral)
			.expect("swap success means collateral >= actual_supply_collateral; qed");
		// refund remain collateral to CDP owner
		if !refund_collateral_amount.is_zero() {
			<T as Config>::CDPTreasury::withdraw_collateral(who, refund_collateral_amount)?;
		}

		// Note: for StableAsset, the swap of cdp treasury is always on `ExactSupply`
		// regardless of this swap_limit params. There will be excess stablecoins that
		// need to be returned to the `who` from cdp treasury account.
		if actual_target_amount > target_stable_amount {
			<T as Config>::CDPTreasury::withdraw_surplus(
				who,
				actual_target_amount.saturating_sub(target_stable_amount),
			)?;
		}

		Ok(())
	}
}

pub struct LiquidateViaAuction<T>(PhantomData<T>);
impl<T: Config> LiquidateCollateral<AccountIdOf<T>, CurrencyIdOf<T>, BalanceOf<T>>
	for LiquidateViaAuction<T>
{
	fn liquidate(
		who: &AccountIdOf<T>,
		_collateral_currency_id: CurrencyIdOf<T>,
		amount: BalanceOf<T>,
		target_stable_amount: BalanceOf<T>,
	) -> DispatchResult {
		<T as Config>::CDPTreasury::create_collateral_auctions(
			amount,
			target_stable_amount,
			who.clone(),
			true,
		)
		.map(|_| ())
	}
}
