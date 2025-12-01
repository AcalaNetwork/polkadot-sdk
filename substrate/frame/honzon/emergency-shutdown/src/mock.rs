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
use crate as pallet_emergency_shutdown;
use frame_support::{
	construct_runtime, derive_impl, ord_parameter_types, parameter_types,
	traits::{
		honzon::{AuctionManager, LockablePrice, MockLiquidationStrategy},
		AsEnsureOriginWithArg, ConstU32, SortedMembers,
	},
	PalletId,
};
use frame_system::{EnsureRoot, EnsureSignedBy};
use sp_runtime::{traits::AccountIdConversion, BuildStorage, DispatchError, DispatchResult};

pub type AccountId = u64;
pub type Balance = u128;
pub type CurrencyId = u32;
pub type AuctionId = u32;

pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;
pub const NATIVE: CurrencyId = 0;
pub const STABLE: CurrencyId = 1;

type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		EmergencyShutdownModule: pallet_emergency_shutdown,
		Balances: pallet_balances,
		Assets: pallet_assets,
		CDPTreasuryModule: pallet_cdp_treasury,
		Loans: pallet_loans,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type AccountId = AccountId;
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig as pallet_balances::pallet::DefaultConfig)]
impl pallet_balances::Config for Test {
	type Balance = Balance;
	type AccountStore = System;
}

pub struct OneMember;
impl SortedMembers<AccountId> for OneMember {
	fn sorted_members() -> Vec<AccountId> {
		vec![1]
	}
}

#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig as pallet_assets::pallet::DefaultConfig)]
impl pallet_assets::Config for Test {
	type Balance = Balance;
	type AssetId = CurrencyId;
	type AssetIdParameter = u32;
	type Currency = Balances;
	type CreateOrigin = AsEnsureOriginWithArg<EnsureSignedBy<OneMember, AccountId>>;
	type ForceOrigin = EnsureRoot<AccountId>;
}

parameter_types! {
	pub const LoansPalletId: PalletId = PalletId(*b"aca/loan");
	pub const GetNativeCurrencyId: CurrencyId = NATIVE;
}

impl pallet_loans::Config for Test {
	type CollateralCurrencyId = GetNativeCurrencyId;
	type RuntimeHoldReason = RuntimeHoldReason;
	type LiquidationStrategy = MockLiquidationStrategy;
	type Currency = Balances;
	type CurrencyId = CurrencyId;
	type RiskManager = ();
	type CDPTreasury = CDPTreasuryModule;
	type PalletId = LoansPalletId;
	type OnUpdateLoan = ();
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
impl pallet_asset_conversion::Swap<AccountId> for MockSwap {
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
	pub const GetStableCurrencyId: CurrencyId = STABLE;
	pub const CDPTreasuryPalletId: PalletId = PalletId(*b"aca/cdpt");
	pub TreasuryAccount: AccountId = PalletId(*b"aca/hztr").into_account_truncating();
}

impl pallet_cdp_treasury::Config for Test {
	type UpdateOrigin = EnsureSignedBy<One, AccountId>;
	type Fungibles = Assets;
	type AssetKind = CurrencyId;
	type AuctionManagerHandler = MockAuctionManager;
	type Balance = Balance;
	type CurrencyId = CurrencyId;
	type MaxAuctionsCount = ConstU32<10_000>;
	type TreasuryAccount = TreasuryAccount;
	type PalletId = CDPTreasuryPalletId;
	type WeightInfo = ();
	type StableCurrencyId = GetStableCurrencyId;
	type CollateralCurrencyId = GetNativeCurrencyId;
	type Swap = MockSwap;
}

impl pallet_emergency_shutdown::Config for Test {
	type PriceSource = MockLockablePrice;
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
