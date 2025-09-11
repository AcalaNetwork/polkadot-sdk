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

//! Mocks for the cdp treasury module.

#![cfg(test)]

use super::*;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	construct_runtime, derive_impl, parameter_types, PalletId,
	traits::{
		AsEnsureOriginWithArg, ConstU128, ConstU32, ConstU64, EitherOfDiverse, Everything,
		Incrementable, InstanceFilter, OnUnbalanced, SortedMembers,
	},
};
use frame_system::{EnsureRoot, EnsureSignedBy};
use pallet_traits::{AggregatedSwapPath, AuctionManager, Swap, SwapLimit};
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::{
	traits::{IdentityLookup},
	BuildStorage, DispatchError, DispatchResult, Permill,
};
use std::cell::RefCell;

pub type AccountId = u128;
pub type BlockNumber = u64;
pub type Amount = i64;
pub type AuctionId = u32;
pub type Balance = u128;

pub type CurrencyId = u32;

pub const ALICE: AccountId = 0;
pub const BOB: AccountId = 1;
pub const CHARLIE: AccountId = 2;
pub const NATIVE: CurrencyId = 0;
pub const STABLE: CurrencyId = 1;

mod cdp_treasury {
	pub use super::super::*;
}

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic
	{
		System: frame_system,
		CDPTreasuryModule: cdp_treasury,
		PalletBalances: pallet_balances,
		Assets: pallet_assets,
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

parameter_types! {
	pub const GetStableCurrencyId: CurrencyId = STABLE;
	pub const GetNativeCurrencyId: CurrencyId = NATIVE;
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

thread_local! {
	pub static TOTAL_COLLATERAL_AUCTION: RefCell<u32> = RefCell::new(0);
	pub static TOTAL_COLLATERAL_IN_AUCTION: RefCell<Balance> = RefCell::new(0);
}

pub fn total_collateral_auction() -> u32 {
	TOTAL_COLLATERAL_AUCTION.with(|v| *v.borrow())
}

pub fn total_collateral_in_auction() -> Balance {
	TOTAL_COLLATERAL_IN_AUCTION.with(|v| *v.borrow())
}

pub struct MockAuctionManager;
impl AuctionManager<AccountId> for MockAuctionManager {
	type AuctionId = AuctionId;
	type Balance = Balance;
	type CurrencyId = CurrencyId;

	fn new_collateral_auction(
		_refund_recipient: &AccountId,
		amount: Self::Balance,
		_target: Self::Balance,
	) -> DispatchResult {
		TOTAL_COLLATERAL_AUCTION.with(|v| *v.borrow_mut() += 1);
		TOTAL_COLLATERAL_IN_AUCTION.with(|v| *v.borrow_mut() += amount);
		Ok(())
	}

	fn cancel_auction(_id: Self::AuctionId) -> DispatchResult {
		unimplemented!()
	}

	fn get_total_collateral_in_auction() -> Self::Balance {
		TOTAL_COLLATERAL_IN_AUCTION.with(|v| *v.borrow())
	}

	fn get_total_target_in_auction() -> Self::Balance {
		unimplemented!()
	}
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
	pub const One: AccountId = 1;
}

parameter_types! {
	pub const CDPTreasuryPalletId: PalletId = PalletId(*b"ac/cdpty");
	pub const TreasuryAccount: AccountId = 10;
}

impl Config for Test {
	type Fungibles = Assets;
	type GetStableCurrencyId = GetStableCurrencyId;
	type GetBaseCurrencyId = GetNativeCurrencyId;
	type AuctionManagerHandler = MockAuctionManager;
	type UpdateOrigin = EitherOfDiverse<EnsureRoot<AccountId>, EnsureSignedBy<OneMember, AccountId>>;
	type MaxAuctionsCount = ConstU32<5>;
	type PalletId = CDPTreasuryPalletId;
	type TreasuryAccount = TreasuryAccount;
	type WeightInfo = ();
	type Balance = Balance;
	type CurrencyId = CurrencyId;
	type Swap = MockSwap;
}

pub struct ExtBuilder {
	balances: Vec<(AccountId, CurrencyId, Balance)>,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {
			balances: vec![
				(ALICE, NATIVE, 100000),
				(ALICE, STABLE, 100000),
				(BOB, NATIVE, 100000),
				(BOB, STABLE, 100000),
				(CHARLIE, NATIVE, 100000),
			],
		}
	}
}

impl ExtBuilder {
	pub fn build(self) -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::<Test>::default()
			.build_storage()
			.unwrap();

		pallet_assets::GenesisConfig::<Test> {
			assets: vec![
				(STABLE, CDPTreasuryModule::account_id(), true, 1),
				(NATIVE, CDPTreasuryModule::account_id(), true, 1),
			],
			metadata: vec![],
			accounts: self.balances.into_iter().map(|(id, asset, balance)| (asset, id, balance)).collect(),
			next_asset_id: None,
		}
		.assimilate_storage(&mut t)
		.unwrap();

		t.into()
	}
}
