use crate::{AccountIdOf, BalanceOf, Config, CurrencyIdOf};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	pallet_prelude::*,
	traits::honzon::{
		CDPTreasury, CDPTreasuryExtended, LiquidateCollateral, PriceProvider, Rate, Ratio,
		SwapLimit,
	},
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{Bounded, One, Saturating},
	FixedPointNumber, FixedPointOperand, FixedU128, RuntimeDebug,
};
/// Compound factor between debit value (FV) and debit balance (PV).
///
/// This represents the accumulated interest rate, where FV = PV * factor.
/// The value must be >= 1, meaning the factor can only increase over time.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct CompoundFactor(FixedU128);
pub enum CompoundFactorError {
	LessThanOne,
}

impl CompoundFactor {
	/// Create a new `CompoundFactor` from a `FixedU128` inner value.
	pub fn try_from_inner(inner: FixedU128) -> Result<Self, CompoundFactorError> {
		if inner < FixedU128::one() {
			return Err(CompoundFactorError::LessThanOne);
		}
		Ok(Self(inner))
	}

	/// Create a new `CompoundFactor` with value 1.
	///
	/// In most cases, it should be the default value for the compound factor.
	pub fn one() -> Self {
		Self(FixedU128::one())
	}

	/// Create a new `CompoundFactor` from a `FixedU128` inner value.
	///
	/// # Safety
	/// The inner value must be >= 1.
	pub const unsafe fn from_inner_unchecked(inner: FixedU128) -> Self {
		Self(inner)
	}

	/// Saturating addition of this factor and another `FixedU128`.
	pub fn saturating_add(self, other: FixedU128) -> Self {
		// Safety: since `other` is always >=0, the result is always >=1, it can be unchecked
		unsafe { Self::from_inner_unchecked(self.0.saturating_add(other)) }
	}

	/// Get the inner `FixedU128` value.
	pub fn into_inner(self) -> FixedU128 {
		self.0
	}

	/// Get the reciprocal of this factor.
	///
	/// Since the factor is always >= 1, the reciprocal always exists.
	pub fn reciprocal(self) -> FixedU128 {
		FixedPointNumber::reciprocal(self.0)
			.expect("compound factor is always >= 1, so reciprocal is always available; qed")
	}

	/// Multiply this factor by an integer value, saturating on overflow.
	pub fn saturating_mul_int<N: FixedPointOperand>(self, n: N) -> N {
		self.0.saturating_mul_int(n)
	}

	/// Saturating multiplication of this factor and another `FixedU128`.
	pub fn saturating_mul(self, rate: FixedU128) -> FixedU128 {
		self.0.saturating_mul(rate)
	}

	/// Create from a rational number, returning `None` if the result would be < 1.
	pub fn checked_from_rational<N: FixedPointOperand, D: FixedPointOperand>(
		n: N,
		d: D,
	) -> Option<Self> {
		FixedPointNumber::checked_from_rational(n, d).and_then(|inner| {
			if inner >= FixedU128::one() {
				Some(Self(inner))
			} else {
				None
			}
		})
	}
}

// Required for storage
impl Encode for CompoundFactor {
	fn encode(&self) -> sp_std::prelude::Vec<u8> {
		self.0.encode()
	}

	fn size_hint(&self) -> usize {
		self.0.size_hint()
	}
}

impl Decode for CompoundFactor {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let inner = FixedU128::decode(input)?;
		if inner >= FixedU128::one() {
			Ok(Self(inner))
		} else {
			Err(codec::Error::from("CompoundFactor must be >= 1"))
		}
	}
}

impl codec::EncodeLike for CompoundFactor {}
impl codec::EncodeLike<FixedU128> for CompoundFactor {}
impl codec::EncodeLike<CompoundFactor> for FixedU128 {}

/// Risk management param
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Clone,
	Copy,
	RuntimeDebug,
	PartialEq,
	Eq,
	Default,
	TypeInfo,
	MaxEncodedLen,
)]
pub struct RiskManagementParams<Balance> {
	/// Maximum total debit value generated from it, when reach the hard
	/// cap, CDP's owner cannot issue more stablecoin under the collateral
	/// type.
	pub maximum_total_debit_value: Balance,

	/// Extra interest rate per sec, `None` value means not set
	pub interest_rate_per_sec: Rate,

	/// Liquidation ratio, when the collateral ratio of
	/// CDP under this collateral type is below the liquidation ratio, this
	/// CDP is unsafe and can be liquidated.
	pub liquidation_ratio: Ratio,

	/// Liquidation penalty rate, when liquidation occurs,
	/// CDP will be deducted an additional penalty base on the product of
	/// penalty rate and debit value.
	pub liquidation_penalty: Rate,

	/// Required collateral ratio, if it's set, cannot adjust the position
	/// of CDP so that the current collateral ratio is lower than the
	/// required collateral ratio. None value means not set
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
