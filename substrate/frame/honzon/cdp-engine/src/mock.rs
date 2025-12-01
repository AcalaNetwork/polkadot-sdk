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
use crate as pallet_cdp_engine;
use frame_support::{
	derive_impl,
	dynamic_params::{dynamic_pallet_params, dynamic_params},
	parameter_types,
	traits::{
		honzon::{
			CDPTreasury as CDPTreasuryT, CDPTreasuryExtended, DebitUnit, EmergencyShutdown,
			Handler, LiquidationTarget, Price, PriceProvider, Rate, Ratio, RiskManager, SwapLimit,
		},
		ConstU128, ConstU32, ConstU64, EnsureOriginWithArg, UnixTime,
	},
};
use pallet_asset_conversion::Swap;
use sp_runtime::{BuildStorage, DispatchResult, FixedU128};

pub type CurrencyId = u32;
type AccountId = u64;
type Block = frame_system::mocking::MockBlock<Test>;
type Balance = u128;

const COLLATERAL_ASSET_ID: CurrencyId = 1;
const STABLE_ASSET_ID: CurrencyId = 2;

#[frame_support::pallet(dev_mode)]
pub mod shutdown_mock {
	use frame_support::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	#[pallet::getter(fn is_shutdown_storage)]
	pub type IsShutdown<T: Config> = StorageValue<_, bool, ValueQuery>;
}

pub fn set_shutdown(value: bool) {
	shutdown_mock::IsShutdown::<Test>::put(value);
}

pub struct DummyOnUpdateLoan;
impl Handler<(AccountId, pallet_loans::BalanceAdjustment<Balance>, Balance)> for DummyOnUpdateLoan {
	fn handle(
		_: &(AccountId, pallet_loans::BalanceAdjustment<Balance>, Balance),
	) -> DispatchResult {
		Ok(())
	}
}

// Configure a mock runtime to test the pallet.
#[frame_support::runtime]
mod runtime {

	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeFreezeReason,
		RuntimeHoldReason,
		RuntimeSlashReason,
		RuntimeLockId,
		RuntimeTask,
		RuntimeViewFunction
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system::Pallet<Test>;

	#[runtime::pallet_index(1)]
	pub type Assets = pallet_assets::Pallet<Test>;

	#[runtime::pallet_index(2)]
	pub type Balances = pallet_balances::Pallet<Test>;

	#[runtime::pallet_index(3)]
	pub type Loans = pallet_loans::Pallet<Test>;

	#[runtime::pallet_index(4)]
	pub type CDPEngine = pallet_cdp_engine::Pallet<Test>;

	#[runtime::pallet_index(5)]
	pub type Parameters = pallet_parameters::Pallet<Test>;

	#[runtime::pallet_index(6)]
	pub type ShutdownMock = shutdown_mock::Pallet<Test>;
}

impl shutdown_mock::Config for Test {}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
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

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig as pallet_balances::pallet::DefaultConfig)]
impl pallet_balances::Config for Test {
	type Balance = Balance;
	type ExistentialDeposit = ConstU128<1>;
	type AccountStore = System;
	type ReserveIdentifier = [u8; 8];
}

#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig as pallet_assets::pallet::DefaultConfig)]
impl pallet_assets::Config for Test {
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
	type RemoveItemsLimit = ConstU32<1000>;
	pallet_assets::runtime_benchmarks_enabled! {
		type BenchmarkHelper = ();
	}
}

#[derive_impl(pallet_parameters::config_preludes::TestDefaultConfig)]
impl pallet_parameters::Config for Test {
	type AdminOrigin = custom_origin::ParamsManager;
}

parameter_types! {
	pub const LoansPalletId: PalletId = PalletId(*b"aca/loan");
	pub const DefaultCompoundFactor: CompoundFactor = unsafe { CompoundFactor::from_inner_unchecked(FixedU128::from_inner(1_000_000_000_000_000_000)) };
	pub const MinimumCollateralAmount: u128 = 100;
	pub const GetNativeCurrencyId: CurrencyId = 1;
	pub const GetStableCurrencyId: CurrencyId = 2;
	pub const MaxSwapSlippageCompareToOracle: Ratio = Ratio::from_rational(10, 100);
}

pub struct MockRiskManager;
impl RiskManager<DebitUnit<Balance>, Balance, FixedU128> for MockRiskManager {
	fn debit_units_to_value(debit_units: DebitUnit<Balance>, _stability_fee: FixedU128) -> Balance {
		debit_units.into_inner()
	}
	fn value_to_debit_units(debit_value: Balance, _stability_fee: FixedU128) -> DebitUnit<Balance> {
		DebitUnit::new(debit_value)
	}
	fn check_position_valid(
		_collateral_balance: Balance,
		_debit_units: DebitUnit<Balance>,
		_debit_interest_rate: FixedU128,
		_check_required_ratio: bool,
	) -> DispatchResult {
		Ok(())
	}
	fn check_debit_cap() -> DispatchResult {
		// Calculate total debit value across all positions
		let mut total_debit_value = Balance::zero();
		pallet_loans::pallet::Positions::<Test>::iter().for_each(|(_who, position)| {
			let debit_value =
				Self::debit_units_to_value(position.debit.units, position.debit.stability_fee);
			total_debit_value = total_debit_value.saturating_add(debit_value);
		});

		// Mock: fail if total debit >= 1000 (test expects error at 1100)
		if total_debit_value >= 1000 {
			return Err(DispatchError::Other("mock exceed debit value cap error"));
		}
		Ok(())
	}
}

pub struct MockLiquidationStrategy;
impl LiquidationTarget<u64, CurrencyId, Balance> for MockLiquidationStrategy {
	fn liquidate(
		_who: &u64,
		_collateral_currency: CurrencyId,
		_collateral_to_sell: Balance,
		_debit_to_cover: Balance,
	) -> Result<(Balance, Balance), DispatchError> {
		Ok((Zero::zero(), Zero::zero()))
	}
}

impl pallet_loans::Config for Test {
	type CurrencyId = CurrencyId;
	type CollateralCurrencyId = GetNativeCurrencyId;
	type Currency = Balances;
	type RuntimeHoldReason = RuntimeHoldReason;
	type RiskManager = CDPEngine;
	type CDPTreasury = MockCDPTreasury;
	type PalletId = LoansPalletId;
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
		shutdown_mock::Pallet::<Test>::is_shutdown_storage()
	}
}

pub struct MockSwap;
impl<AccountId> Swap<AccountId> for MockSwap
where
	AccountId: Clone,
{
	type Balance = Balance;
	type AssetKind = CurrencyId;
	fn max_path_len() -> u32 {
		8
	}

	fn swap_exact_tokens_for_tokens(
		_sender: AccountId,
		_path: Vec<Self::AssetKind>,
		amount_in: Self::Balance,
		_amount_out_min: Option<Self::Balance>,
		_send_to: AccountId,
		_keep_alive: bool,
	) -> Result<Self::Balance, DispatchError> {
		// Return a mock amount out
		Ok(amount_in)
	}

	fn swap_tokens_for_exact_tokens(
		_sender: AccountId,
		_path: Vec<Self::AssetKind>,
		amount_out: Self::Balance,
		_amount_in_max: Option<Self::Balance>,
		_send_to: AccountId,
		_keep_alive: bool,
	) -> Result<Self::Balance, DispatchError> {
		// Return a mock amount in
		Ok(amount_out)
	}
}

// Define dynamic runtime parameters for tests and expose them via pallet-parameters.
#[dynamic_params(RuntimeParameters, pallet_parameters::Parameters::<Test>)]
pub mod dynamic_params {
	use super::*;

	// Parameters for the CDP Engine pallet used in tests.
	#[dynamic_pallet_params]
	#[codec(index = 0)]
	pub mod cdp_engine {
		/// Risk management parameters used by CDP Engine in tests.
		#[codec(index = 0)]
		pub static RiskManagementParams: pallet_cdp_engine::RiskManagementParams<Balance> =
			pallet_cdp_engine::RiskManagementParams {
				// 1e18 as a simple hard cap for tests
				maximum_total_debit_value: 1_000_000_000_000_000_000u128,
				// default extra interest rate per sec (0)
				interest_rate_per_sec: Rate::from_rational(1, 10000),
				// liquidation ratio 1.5
				liquidation_ratio: Ratio::from_rational(15, 10),
				// default liquidation penalty factor 0
				liquidation_penalty: Rate::zero(),
				// required collateral ratio is not set
				required_collateral_ratio: None,
			};
	}
}

mod custom_origin {
	use super::*;
	pub struct ParamsManager;
	impl EnsureOriginWithArg<RuntimeOrigin, RuntimeParametersKey> for ParamsManager {
		type Success = ();

		fn try_origin(
			origin: RuntimeOrigin,
			_key: &RuntimeParametersKey,
		) -> Result<Self::Success, RuntimeOrigin> {
			ensure_root(origin.clone()).map_err(|_| origin)
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn try_successful_origin(_key: &RuntimeParametersKey) -> Result<RuntimeOrigin, ()> {
			Ok(RuntimeOrigin::root())
		}
	}
}

impl pallet_cdp_engine::Config for Test {
	type AssetKind = CurrencyId;
	type DefaultCompoundFactor = DefaultCompoundFactor;
	type MinimumDebitValue = ConstU128<100>;
	type MinimumCollateralAmount = MinimumCollateralAmount;
	type GetNativeCurrencyId = GetNativeCurrencyId;
	type GetStableCurrencyId = GetStableCurrencyId;
	type MaxSwapSlippageCompareToOracle = MaxSwapSlippageCompareToOracle;
	// Read risk params via pallet-parameters dynamic key.
	type RiskManagementParams = dynamic_params::cdp_engine::RiskManagementParams;
	type CDPTreasury = MockCDPTreasury;
	type PriceSource = MockPriceProvider;
	type UnsignedPriority = ConstU64<100>;
	type EmergencyShutdown = MockEmergencyShutdown;
	type UnixTime = MockUnixTime;
	type Currency = Balances;
	type Tokens = Assets;
	type Swap = MockSwap;
	type PalletId = CDPEnginePalletId;
	type WeightInfo = ();
	type VaultId = u64;
}

parameter_types! {
	pub const CDPEnginePalletId: frame_support::PalletId = frame_support::PalletId(*b"aca/cdpe");
	pub const ConstRatio150: Ratio = Ratio::from_inner(1_500_000_000_000_000_000u128); // 1.5
}

pub struct MockCDPTreasury;
impl<AccountId: Clone + Default> CDPTreasuryT<AccountId> for MockCDPTreasury {
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

	storage.into()
}
