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

//! Mocks for the emergency shutdown module.

#![cfg(test)]

use super::*;
use frame_support::{
	construct_runtime, derive_impl, ord_parameter_types, parameter_types,
	traits::{
		AsEnsureOriginWithArg, ConstU128, ConstU32, ConstU64, Everything, Nothing, SortedMembers,
	},
	PalletId,
};
use frame_system::{EnsureRoot, EnsureSignedBy};
use pallet_traits::{
	AggregatedSwapPath, AuctionManager, CDPTreasury, LockablePrice, PriceProvider, RiskManager,
	Swap, SwapLimit,
};
use sp_runtime::{
	traits::{AccountIdConversion, IdentityLookup},
	BuildStorage, DispatchError, DispatchResult, FixedU128,
};

pub type AccountId = u128;
pub type BlockNumber = u64;
pub type Balance = u128;
pub type CurrencyId = u32;
pub type Amount = i128;
pub type AuctionId = u32;

pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;
pub const NATIVE: CurrencyId = 0;
pub const STABLE: CurrencyId = 1;

mod emergency_shutdown {
	pub use super::super::*;
}

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		EmergencyShutdownModule: emergency_shutdown,
		PalletBalances: pallet_balances,
		Assets: pallet_assets,
		CDPTreasuryModule: pallet_cdp_treasury,
		Loans: pallet_loans,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type BaseCallFilter = Everything;
	type SystemWeightInfo = ();
	type PalletInfo = PalletInfo;
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
}

impl pallet_balances::Config for Test {
	type Balance = Balance;
	type DustRemoval = ();
	type RuntimeEvent = RuntimeEvent;
	type ExistentialDeposit = ConstU128<1>;
	type AccountStore = System;
	type MaxLocks = ();
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type WeightInfo = ();
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type FreezeIdentifier = ();
	type MaxFreezes = ();
	type DoneSlashHandler = ();
}

pub struct OneMember;
impl SortedMembers<AccountId> for OneMember {
	fn sorted_members() -> Vec<AccountId> {
		vec![1]
	}
}

impl pallet_assets::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type AssetId = CurrencyId;
	type AssetIdParameter = u32;
	type Currency = PalletBalances;
	type CreateOrigin = AsEnsureOriginWithArg<EnsureSignedBy<OneMember, AccountId>>;
	type ForceOrigin = EnsureRoot<AccountId>;
	type AssetDeposit = ConstU128<1>;
	type AssetAccountDeposit = ConstU128<1>;
	type MetadataDepositBase = ConstU128<1>;
	type MetadataDepositPerByte = ConstU128<1>;
	type ApprovalDeposit = ConstU128<1>;
	type StringLimit = ConstU32<50>;
	type Freezer = ();
	type Extra = ();
	type WeightInfo = ();
	type RemoveItemsLimit = ConstU32<1000>;
	type CallbackHandle = ();
	type Holder = ();
}

pub struct MockRiskManager;
impl RiskManager<AccountId, CurrencyId, Balance, Balance> for MockRiskManager {
	fn get_debit_value(_currency_id: CurrencyId, debit_balance: Balance) -> Balance {
		debit_balance
	}

	fn check_position_valid(
		_currency_id: CurrencyId,
		_collateral_balance: Balance,
		_debit_balance: Balance,
		_check_required_ratio: bool,
	) -> DispatchResult {
		Ok(())
	}

	fn check_debit_cap(_currency_id: CurrencyId, _total_debit_balance: Balance) -> DispatchResult {
		Ok(())
	}
}

parameter_types! {
	pub const LoansPalletId: PalletId = PalletId(*b"aca/loan");
	pub const GetNativeCurrencyId: CurrencyId = NATIVE;
}

impl pallet_loans::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Currency = PalletBalances;
	type CurrencyId = CurrencyId;
	type RiskManager = MockRiskManager;
	type CDPTreasury = CDPTreasuryModule;
	type PalletId = LoansPalletId;
	type CollateralCurrencyId = GetNativeCurrencyId;
	type OnUpdateLoan = ();
}

pub struct MockPriceProvider;
impl PriceProvider<CurrencyId> for MockPriceProvider {
	fn get_price(_currency_id: CurrencyId) -> Option<FixedU128> {
		Some(FixedU128::from_inner(100))
	}
}

pub struct MockLockablePrice;
impl LockablePrice<CurrencyId> for MockLockablePrice {
	fn lock_price(_currency_id: CurrencyId) -> DispatchResult {
		Ok(())
	}

	fn unlock_price(_currency_id: CurrencyId) -> DispatchResult {
		Ok(())
	}
}

pub struct MockAuctionManager;
impl AuctionManager<AccountId> for MockAuctionManager {
	type Balance = Balance;
	type CurrencyId = CurrencyId;
	type AuctionId = AuctionId;

	fn new_collateral_auction(
		_refund_recipient: &AccountId,
		_currency_id: Self::CurrencyId,
		_amount: Self::Balance,
		_target: Self::Balance,
	) -> DispatchResult {
		unimplemented!()
	}

	fn cancel_auction(_id: Self::AuctionId) -> DispatchResult {
		unimplemented!()
	}

	fn get_total_target_in_auction() -> Self::Balance {
		unimplemented!()
	}

	fn get_total_collateral_in_auction(_id: Self::CurrencyId) -> Self::Balance {
		Default::default()
	}
}

ord_parameter_types! {
	pub const One: AccountId = 1;
}

pub struct MockSwap;
impl Swap<AccountId, Balance, CurrencyId> for MockSwap {
	fn swap(
		_who: &AccountId,
		_from: CurrencyId,
		_to: CurrencyId,
		_limit: SwapLimit<Balance>,
	) -> Result<(Balance, Balance), DispatchError> {
		Ok((0, 0))
	}

	fn get_swap_amount(
		_from: CurrencyId,
		_to: CurrencyId,
		_limit: SwapLimit<Balance>,
	) -> Option<(Balance, Balance)> {
		None
	}

	fn swap_by_path(
		_who: &AccountId,
		_path: &[CurrencyId],
		_limit: SwapLimit<Balance>,
	) -> Result<(Balance, Balance), DispatchError> {
		Ok((0, 0))
	}

	fn swap_by_aggregated_path<StableAssetPoolId, PoolTokenIndex>(
		_who: &AccountId,
		_path: &[AggregatedSwapPath<CurrencyId, StableAssetPoolId, PoolTokenIndex>],
		_limit: SwapLimit<Balance>,
	) -> Result<(Balance, Balance), DispatchError> {
		Ok((0, 0))
	}
}

parameter_types! {
	pub const GetStableCurrencyId: CurrencyId = STABLE;
	pub const CDPTreasuryPalletId: PalletId = PalletId(*b"aca/cdpt");
	pub TreasuryAccount: AccountId = PalletId(*b"aca/hztr").into_account_truncating();
}

impl pallet_cdp_treasury::Config for Test {
	type UpdateOrigin = EnsureSignedBy<One, AccountId>;
	type Fungibles = Assets;
	type AuctionManagerHandler = MockAuctionManager;
	type Balance = Balance;
	type CurrencyId = CurrencyId;
	type MaxAuctionsCount = ConstU32<10_000>;
	type TreasuryAccount = TreasuryAccount;
	type PalletId = CDPTreasuryPalletId;
	type WeightInfo = ();
	type GetStableCurrencyId = GetStableCurrencyId;
	type GetBaseCurrencyId = GetNativeCurrencyId;
	type Swap = MockSwap;
}

parameter_types! {
	pub const CollateralCurrencyId: CurrencyId = NATIVE;
}

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type CollateralCurrencyId = CollateralCurrencyId;
	type PriceSource = MockLockablePrice;
	type CDPTreasury = CDPTreasuryModule;
	type AuctionManagerHandler = MockAuctionManager;
	type ShutdownOrigin = EnsureSignedBy<One, AccountId>;
	type WeightInfo = ();
}

pub struct ExtBuilder {
	balances: Vec<(AccountId, CurrencyId, Balance)>,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self { balances: vec![(ALICE, NATIVE, 1000), (BOB, NATIVE, 1000)] }
	}
}

impl ExtBuilder {
	pub fn build(self) -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

		pallet_assets::GenesisConfig::<Test> {
			assets: vec![
				(STABLE, CDPTreasuryModule::account_id(), true, 1),
				(NATIVE, CDPTreasuryModule::account_id(), true, 1),
			],
			metadata: vec![],
			accounts: self
				.balances
				.into_iter()
				.map(|(id, asset, balance)| (asset, id, balance))
				.collect(),
			next_asset_id: None,
		}
		.assimilate_storage(&mut t)
		.unwrap();

		t.into()
	}
}
