// This file is part of Acala.

// Copyright (C) 2020-2025 Acala Foundation.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Mock runtime for CDP Engine pallet

use super::*;
use core::sync::atomic::{AtomicBool, Ordering};
use frame_support::{
	construct_runtime, parameter_types,
	traits::{ConstU128, ConstU32, ConstU64, Get, UnixTime},
};
use pallet_traits::{
	AggregatedSwapPath, CDPTreasury as CDPTreasuryT, CDPTreasuryExtended, DEXManager, EmergencyShutdown,
	ExchangeRate, Handler, LiquidationTarget, Position, Price, PriceProvider, Rate, Ratio, RiskManager,
	Swap, SwapLimit,
};
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup, Zero},
	BuildStorage, DispatchError, DispatchResult,
};
use sp_std::marker::PhantomData;

pub type CurrencyId = u32;
type AccountId = u64;
type Block = frame_system::mocking::MockBlock<Test>;
type Balance = u128;
type Amount = i128;

const COLLATERAL_ASSET_ID: CurrencyId = 1;
const STABLE_ASSET_ID: CurrencyId = 2;

static IS_SHUTDOWN: AtomicBool = AtomicBool::new(false);

pub fn set_shutdown(value: bool) {
	IS_SHUTDOWN.store(value, Ordering::SeqCst);
}

pub struct DummyOnUpdateLoan;
impl Handler<(AccountId, Amount, Balance)> for DummyOnUpdateLoan {
	fn handle(_: &(AccountId, Amount, Balance)) -> DispatchResult {
		Ok(())
	}
}

// Configure a mock runtime to test the pallet.
construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Assets: pallet_assets,
		Balances: pallet_balances,
		Loans: pallet_loans,
		CDPEngine: crate,
	}
);

impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = sp_core::H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = ConstU64<250>;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<u128>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
	type RuntimeTask = ();
	type ExtensionsWeightInfo = ();
	type SingleBlockMigrations = ();
	type MultiBlockMigrator = ();
	type PreInherents = ();
	type PostInherents = ();
	type PostTransactions = ();
}

impl<C> frame_system::offchain::CreateTransactionBase<C> for Test
where
	RuntimeCall: From<C>,
{
	type Extrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
	type RuntimeCall = RuntimeCall;
}

impl<C> frame_system::offchain::CreateBare<C> for Test
where
	RuntimeCall: From<C>,
{
	fn create_bare(call: Self::RuntimeCall) -> Self::Extrinsic {
		frame_system::mocking::MockUncheckedExtrinsic::<Test>::new_bare(call)
	}
}

/// Mock UnixTime implementation that returns a fixed timestamp
pub struct MockUnixTime;
impl UnixTime for MockUnixTime {
	fn now() -> core::time::Duration {
		// Return a fixed timestamp for testing
		core::time::Duration::from_secs(1234567890)
	}
}

impl pallet_balances::Config for Test {
	type Balance = Balance;
	type DustRemoval = ();
	type RuntimeEvent = RuntimeEvent;
	type ExistentialDeposit = ConstU128<1>;
	type AccountStore = System;
	type WeightInfo = ();
	type MaxLocks = ();
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type RuntimeHoldReason = pallet_loans::HoldReason;
	type RuntimeFreezeReason = ();
	type FreezeIdentifier = ();
	type MaxFreezes = ();
	type DoneSlashHandler = ();
}

impl pallet_assets::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type AssetId = CurrencyId;
	type AssetIdParameter = CurrencyId;
	type Currency = Balances;
	type CreateOrigin = frame_system::EnsureSigned<AccountId>;
	type ForceOrigin = frame_system::EnsureRoot<AccountId>;
	type AssetDeposit = ConstU128<0>;
	type AssetAccountDeposit = ConstU128<0>;
	type MetadataDepositBase = ConstU128<0>;
	type MetadataDepositPerByte = ConstU128<0>;
	type ApprovalDeposit = ConstU128<0>;
	type StringLimit = ConstU32<64>;
	type Freezer = ();
	type Extra = ();
	type WeightInfo = ();
	type RemoveItemsLimit = ConstU32<1000>;
	type CallbackHandle = ();
	type Holder = ();
}

parameter_types! {
	pub const LoansPalletId: PalletId = PalletId(*b"aca/loan");
	pub const DefaultDebitExchangeRate: ExchangeRate = ExchangeRate::from_inner(1_000_000_000_000_000_000);
	pub const MinimumCollateralAmount: u128 = 100;
	pub const GetNativeCurrencyId: CurrencyId = 1;
	pub const GetStableCurrencyId: CurrencyId = 2;
	pub const MaxSwapSlippageCompareToOracle: Ratio = Ratio::from_rational(10, 100);
}

pub struct DefaultPenalty;
impl Get<FractionalRate> for DefaultPenalty {
	fn get() -> FractionalRate {
		FractionalRate::default()
	}
}

pub struct MockRiskManager;
impl RiskManager<u64, CurrencyId, Balance, Balance> for MockRiskManager {
	fn get_debit_value(_currency_id: CurrencyId, debit_balance: Balance) -> Balance {
		debit_balance
	}

	fn check_position_valid(
		_currency_id: CurrencyId,
		collateral_balance: Balance,
		debit_balance: Balance,
		check_required_ratio: bool,
	) -> DispatchResult {
		if debit_balance.is_zero() {
			return Ok(());
		}
		if check_required_ratio {
			let collateral_value = collateral_balance.saturating_mul(100);
			let required_value = debit_balance.saturating_mul(150);
			if collateral_value < required_value {
				return Err(Error::<Test>::BelowRequiredCollateralRatio.into());
			}
		}
		Ok(())
	}

	fn check_debit_cap(_currency_id: CurrencyId, _total_debit_balance: Balance) -> DispatchResult {
		Ok(())
	}
}

pub struct MockLiquidationStrategy;
impl LiquidationTarget<u64, CurrencyId, Balance> for MockLiquidationStrategy {
	fn liquidate(
		_who: &u64,
		_currency_id: CurrencyId,
		_collateral_to_sell: Balance,
		_debit_to_cover: Balance,
	) -> Result<(Balance, Balance), DispatchError> {
		Ok((Zero::zero(), Zero::zero()))
	}
}

impl pallet_loans::Config for Test {
	type Amount = Amount;
	type Currency = Balances;
	type RuntimeHoldReason = pallet_loans::HoldReason;
	type CurrencyId = CurrencyId;
	type RiskManager = MockRiskManager;
	type CDPTreasury = MockCDPTreasury;
	type PalletId = LoansPalletId;
	type CollateralCurrencyId = GetNativeCurrencyId;
	type OnUpdateLoan = DummyOnUpdateLoan;
	type LiquidationStrategy = MockLiquidationStrategy;
}

pub struct MockPriceProvider;
impl PriceProvider<CurrencyId> for MockPriceProvider {
	fn get_relative_price(_base: CurrencyId, _quote: CurrencyId) -> Option<Price> {
		Some(Price::from_inner(1_000_000_000_000_000_000))
	}

	fn get_price(_currency_id: CurrencyId) -> Option<Price> {
		Some(Price::from_inner(1_000_000_000_000_000_000))
	}
}

pub struct MockEmergencyShutdown;
impl EmergencyShutdown for MockEmergencyShutdown {
	fn is_shutdown() -> bool {
		IS_SHUTDOWN.load(Ordering::SeqCst)
	}
}

pub struct MockDEXManager;
impl<AccountId, Balance, CurrencyId> DEXManager<AccountId, Balance, CurrencyId> for MockDEXManager
where
	Balance: From<u128> + Copy,
{
	fn get_liquidity_pool(
		_currency_id_a: CurrencyId,
		_currency_id_b: CurrencyId,
	) -> (Balance, Balance) {
		(Balance::from(1u128), Balance::from(1u128))
	}

	fn get_liquidity_token_address(
		_currency_id_a: CurrencyId,
		_currency_id_b: CurrencyId,
	) -> Option<sp_core::H160> {
		None
	}

	fn get_swap_amount(
		_path: &[CurrencyId],
		_limit: SwapLimit<Balance>,
	) -> Option<(Balance, Balance)> {
		let _ = (_path, _limit);
		Some((Balance::from(1u128), Balance::from(1u128)))
	}

	fn get_best_price_swap_path(
		_supply_currency_id: CurrencyId,
		_target_currency_id: CurrencyId,
		_limit: SwapLimit<Balance>,
		_alternative_path_joint_list: Vec<Vec<CurrencyId>>,
	) -> Option<(Vec<CurrencyId>, Balance, Balance)> {
		Some((Vec::new(), Balance::from(1u128), Balance::from(1u128)))
	}

	fn swap_with_specific_path(
		_who: &AccountId,
		_path: &[CurrencyId],
		_limit: SwapLimit<Balance>,
	) -> Result<(Balance, Balance), DispatchError> {
		Ok((Balance::from(1u128), Balance::from(1u128)))
	}

	fn add_liquidity(
		_who: &AccountId,
		_currency_id_a: CurrencyId,
		_currency_id_b: CurrencyId,
		_max_amount_a: Balance,
		_max_amount_b: Balance,
		_min_share_increment: Balance,
		_stake_increment_share: bool,
	) -> Result<(Balance, Balance, Balance), DispatchError> {
		Ok((Balance::from(0u128), Balance::from(0u128), Balance::from(0u128)))
	}

	fn remove_liquidity(
		_who: &AccountId,
		_currency_id_a: CurrencyId,
		_currency_id_b: CurrencyId,
		_remove_share: Balance,
		_min_withdrawn_a: Balance,
		_min_withdrawn_b: Balance,
		_by_unstake: bool,
	) -> Result<(Balance, Balance), DispatchError> {
		Ok((Balance::from(0u128), Balance::from(0u128)))
	}
}

impl<AccountId, Balance, CurrencyId> Swap<AccountId, Balance, CurrencyId> for MockDEXManager
where
	Balance: From<u128> + Copy,
	CurrencyId: Clone,
{
	fn get_swap_amount(
		supply_currency_id: CurrencyId,
		target_currency_id: CurrencyId,
		limit: SwapLimit<Balance>,
	) -> Option<(Balance, Balance)> {
		let _ = (supply_currency_id, target_currency_id, limit);
		Some((Balance::from(1u128), Balance::from(1u128)))
	}

	fn swap(
		_who: &AccountId,
		_supply_currency_id: CurrencyId,
		_target_currency_id: CurrencyId,
		_limit: SwapLimit<Balance>,
	) -> Result<(Balance, Balance), DispatchError> {
		Ok((Balance::from(1u128), Balance::from(1u128)))
	}

	fn swap_by_path(
		_who: &AccountId,
		swap_path: &[CurrencyId],
		_limit: SwapLimit<Balance>,
	) -> Result<(Balance, Balance), DispatchError> {
		let _ = (swap_path, _limit);
		Ok((Balance::from(1u128), Balance::from(1u128)))
	}

	fn swap_by_aggregated_path<StableAssetPoolId, PoolTokenIndex>(
		_who: &AccountId,
		swap_path: &[AggregatedSwapPath<CurrencyId, StableAssetPoolId, PoolTokenIndex>],
		_limit: SwapLimit<Balance>,
	) -> Result<(Balance, Balance), DispatchError> {
		let _ = (swap_path, _limit);
		Ok((Balance::from(1u128), Balance::from(1u128)))
	}
}

impl Config for Test {
	type UpdateOrigin = frame_system::EnsureRoot<Self::AccountId>;
	type DefaultLiquidationRatio = ConstRatio150;
	type DefaultDebitExchangeRate = DefaultDebitExchangeRate;
	type DefaultLiquidationPenalty = DefaultPenalty;
	type MinimumDebitValue = ConstU128<100>;
	type MinimumCollateralAmount = MinimumCollateralAmount;
	type GetNativeCurrencyId = GetNativeCurrencyId;
	type GetStableCurrencyId = GetStableCurrencyId;
	type MaxSwapSlippageCompareToOracle = MaxSwapSlippageCompareToOracle;
	type CDPTreasury = MockCDPTreasury;
	type PriceSource = MockPriceProvider;
	type UnsignedPriority = ConstU64<100>;
	type EmergencyShutdown = MockEmergencyShutdown;
	type UnixTime = MockUnixTime;
	type Currency = Balances;
	type Tokens = Assets;
	type DEX = MockDEXManager;
	type Swap = MockDEXManager;
	type PalletId = CDPEnginePalletId;
	type WeightInfo = ();
}

parameter_types! {
	pub const CDPEnginePalletId: frame_support::PalletId = frame_support::PalletId(*b"aca/cdpe");
	pub const ConstRatio150: Ratio = Ratio::from_inner(1_500_000_000_000_000_000u128); // 1.5
}

pub struct MockCDPTreasury;
impl<AccountId: Clone + Default> CDPTreasuryT<AccountId> for MockCDPTreasury {
	type CurrencyId = u32;
	type Balance = Balance;

	fn account_id() -> AccountId {
		Default::default()
	}

	fn get_debit_pool() -> Self::Balance {
		Zero::zero()
	}

	fn get_surplus_pool() -> Self::Balance {
		Zero::zero()
	}

	fn get_total_collaterals() -> Self::Balance {
		Zero::zero()
	}

	fn get_debit_proportion(_amount: Self::Balance) -> Ratio {
		Ratio::zero()
	}

	fn on_system_debit(_amount: Self::Balance) -> DispatchResult {
		Ok(())
	}

	fn deposit_surplus(_from: &AccountId, _surplus: Self::Balance) -> DispatchResult {
		Ok(())
	}

	fn deposit_collateral(_from: &AccountId, _amount: Self::Balance) -> DispatchResult {
		Ok(())
	}

	fn pay_surplus(_amount: Self::Balance) -> DispatchResult {
		Ok(())
	}

	fn refund_surplus(_amount: Self::Balance) -> DispatchResult {
		Ok(())
	}

	fn withdraw_collateral(_to: &AccountId, _amount: Self::Balance) -> DispatchResult {
		Ok(())
	}

	fn withdraw_surplus(_to: &AccountId, _amount: Self::Balance) -> DispatchResult {
		Ok(())
	}

	fn issue_debit(_who: &AccountId, _amount: Self::Balance, _backed: bool) -> DispatchResult {
		Ok(())
	}

	fn burn_debit(_who: &AccountId, _amount: Self::Balance) -> DispatchResult {
		Ok(())
	}

	fn on_system_surplus(_amount: Self::Balance) -> DispatchResult {
		Ok(())
	}
}

impl<AccountId: Clone + Default> CDPTreasuryExtended<AccountId> for MockCDPTreasury {
	fn swap_collateral_to_stable(
		_swap_limit: SwapLimit<Self::Balance>,
		_collateral_in_auction: bool,
	) -> Result<(Self::Balance, Self::Balance), DispatchError> {
		Ok((Zero::zero(), Zero::zero()))
	}

	fn create_collateral_auctions(
		_amount: Self::Balance,
		_target: Self::Balance,
		_refund_receiver: AccountId,
		_split: bool,
	) -> Result<u32, DispatchError> {
		Ok(0)
	}

	fn max_auction() -> u32 {
		0
	}
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

	crate::GenesisConfig::<Test> {
		collateral_params: (
			Some(Rate::from_inner(1_000_000_000)),
			Some(Ratio::from_inner(1_500_000_000_000_000_000u128)), // 1.5
			Some(Rate::from_inner(1_000_000_000_000_000_000)),
			Some(Ratio::from_inner(2_000_000_000_000_000_000u128)), // 2.0
			1000000000000000000u128,
		),
		_phantom: PhantomData,
	}
	.assimilate_storage(&mut storage)
	.unwrap();

	pallet_assets::GenesisConfig::<Test> {
		assets: vec![(COLLATERAL_ASSET_ID, 1, true, 1), (STABLE_ASSET_ID, 1, true, 1)],
		metadata: vec![],
		accounts: vec![
			(COLLATERAL_ASSET_ID, 1, 1_000_000u128),
			(COLLATERAL_ASSET_ID, 2, 1_000_000u128),
			(STABLE_ASSET_ID, 1, 1_000_000u128),
			(STABLE_ASSET_ID, 2, 1_000_000u128),
		],
		next_asset_id: None,
	}
	.assimilate_storage(&mut storage)
	.unwrap();

	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(1, 1_000_000u128), (2, 1_000_000u128)],
		dev_accounts: None,
	}
	.assimilate_storage(&mut storage)
	.unwrap();

	let mut ext = sp_io::TestExternalities::from(storage);
	ext.execute_with(|| {
		set_shutdown(false);
		pallet_loans::TotalPositions::<Test>::put(Position::default());
		pallet_loans::TotalDebitByStabilityFee::<Test>::remove_all(None);
		pallet_loans::Positions::<Test>::remove_all(None);
	});

	ext
}
