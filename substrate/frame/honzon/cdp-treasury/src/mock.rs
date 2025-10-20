// This file is part of Substrate.

// Copyright (C) 2020-2025 Acala Foundation.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Mocks for the cdp treasury module.

#![cfg(test)]

use super::*;
use crate as pallet_cdp_treasury;
use frame_support::{
	construct_runtime, derive_impl, parameter_types,
	traits::{
		honzon::AuctionManager, AsEnsureOriginWithArg, ConstU128, ConstU32, EitherOfDiverse,
		SortedMembers,
	},
	PalletId,
};
use frame_system::{EnsureRoot, EnsureSignedBy};
use pallet_asset_conversion::Swap;
use sp_runtime::{traits::IdentityLookup, BuildStorage, DispatchError, DispatchResult};
use std::cell::RefCell;

pub type AccountId = u128;
pub type AuctionId = u32;
pub type Balance = u128;

pub type CurrencyId = u32;

pub const ALICE: AccountId = 0;
pub const BOB: AccountId = 1;
pub const CHARLIE: AccountId = 2;
pub const NATIVE: CurrencyId = 0;
pub const STABLE: CurrencyId = 1;

construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		CDPTreasuryModule: pallet_cdp_treasury,
		PalletBalances: pallet_balances,
		Assets: pallet_assets,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = frame_system::mocking::MockBlock<Test>;
	type AccountData = pallet_balances::AccountData<Balance>;
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
	pallet_assets::runtime_benchmarks_enabled! {
		type BenchmarkHelper = ();
	}
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
		_collateral_currency: Self::CurrencyId,
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

	fn get_total_collateral_in_auction(_collateral_currency: Self::CurrencyId) -> Self::Balance {
		TOTAL_COLLATERAL_IN_AUCTION.with(|v| *v.borrow())
	}

	fn get_total_target_in_auction() -> Self::Balance {
		unimplemented!()
	}
}

pub struct MockSwap;
impl Swap<AccountId> for MockSwap {
	type Balance = Balance;
	type AssetKind = CurrencyId;

	fn max_path_len() -> u32 {
		2
	}

	fn swap_exact_tokens_for_tokens(
		_sender: AccountId,
		_path: Vec<Self::AssetKind>,
		_amount_in: Self::Balance,
		_amount_out_min: Option<Self::Balance>,
		_send_to: AccountId,
		_keep_alive: bool,
	) -> Result<Self::Balance, DispatchError> {
		Ok(0)
	}

	fn swap_tokens_for_exact_tokens(
		_sender: AccountId,
		_path: Vec<Self::AssetKind>,
		_amount_out: Self::Balance,
		_amount_in_max: Option<Self::Balance>,
		_send_to: AccountId,
		_keep_alive: bool,
	) -> Result<Self::Balance, DispatchError> {
		Ok(0)
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
	type AssetKind = CurrencyId;
	type StableCurrencyId = GetStableCurrencyId;
	type CollateralCurrencyId = GetNativeCurrencyId;
	type AuctionManagerHandler = MockAuctionManager;
	type UpdateOrigin =
		EitherOfDiverse<EnsureRoot<AccountId>, EnsureSignedBy<OneMember, AccountId>>;
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
