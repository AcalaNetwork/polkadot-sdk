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

//! Mocks for the issuance buffer module.

#![cfg(test)]

use super::*;
use frame_support::{
	construct_runtime, derive_impl, parameter_types,
	traits::{ConstU128, ConstU32, Everything, fungibles::{Inspect, Mutate, Unbalanced}, tokens::{ConversionToAssetBalance, DepositConsequence, WithdrawConsequence, Preservation, Fortitude, Precision, Provenance}},
	PalletId,
};
use frame_system::EnsureRoot;
use pallet_traits::{PriceProvider, AuctionManager, Swap, AggregatedSwapPath};
use sp_runtime::{
	BuildStorage, FixedU128, DispatchError, DispatchResult,
};
use sp_core::H256;
use sp_std::{result, marker::PhantomData};

pub type AccountId = u64;
pub type Balance = u128;
pub type CurrencyId = u32;

pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;
pub const NATIVE: CurrencyId = 0;
pub const STABLE: CurrencyId = 1;

mod issuance_buffer {
	pub use super::super::*;
}

type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		IssuanceBuffer: issuance_buffer,
		PalletBalances: pallet_balances,
		CDPTreasury: pallet_cdp_treasury,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type BaseCallFilter = Everything;
	type SystemWeightInfo = ();
	type PalletInfo = PalletInfo;
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
    type AccountId = AccountId;
    type Nonce = u64;
    type Hash = H256;
    type BlockWeights = ();
    type BlockLength = ();
    type DbWeight = ();
    type SS58Prefix = ();
	type Version = ();
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
	type FreezeIdentifier = [u8; 8];
	type MaxFreezes = ConstU32<10>;
    type DoneSlashHandler = ();
}

pub struct MockFungibles;
impl Inspect<AccountId> for MockFungibles {
    type AssetId = CurrencyId;
    type Balance = Balance;
    fn total_issuance(_asset: Self::AssetId) -> Self::Balance { 0 }
    fn minimum_balance(_asset: Self::AssetId) -> Self::Balance { 0 }
    fn balance(_asset: Self::AssetId, _who: &AccountId) -> Self::Balance { 0 }
    fn reducible_balance(_asset: Self::AssetId, _who: &AccountId, _preservation: Preservation, _fortitude: Fortitude) -> Self::Balance { 0 }
    fn can_deposit(_asset: Self::AssetId, _who: &AccountId, _amount: Self::Balance, _provenance: Provenance) -> DepositConsequence { DepositConsequence::Success }
    fn can_withdraw(_asset: Self::AssetId, _who: &AccountId, _amount: Self::Balance) -> WithdrawConsequence<Self::Balance> { WithdrawConsequence::Success }
    fn asset_exists(_asset: Self::AssetId) -> bool { true }
    fn total_balance(_asset: Self::AssetId, _who: &AccountId) -> Self::Balance { 0 }
}
impl Unbalanced<AccountId> for MockFungibles {
    fn handle_raw_dust(_asset: Self::AssetId, _dust: Self::Balance) {}
    fn write_balance(_asset: Self::AssetId, _who: &AccountId, _amount: Self::Balance) -> Result<Option<Self::Balance>, DispatchError> { Ok(None) }
    fn set_total_issuance(_asset: Self::AssetId, _amount: Self::Balance) {}
    fn handle_dust(_dust: frame_support::traits::tokens::fungibles::Dust<AccountId, Self>) {}
}
impl Mutate<AccountId> for MockFungibles {
    fn mint_into(_asset: Self::AssetId, _who: &AccountId, amount: Self::Balance) -> Result<Self::Balance, DispatchError> { Ok(amount) }
    fn burn_from(_asset: Self::AssetId, _who: &AccountId, _amount: Self::Balance, _preservation: Preservation, _precision: Precision, _fortitude: Fortitude) -> Result<Self::Balance, DispatchError> { Ok(0) }
}

pub struct MockAuctionManager<AccountId>(PhantomData<AccountId>);
impl<AccountId> AuctionManager<AccountId> for MockAuctionManager<AccountId> {
	type CurrencyId = CurrencyId;
	type Balance = Balance;
	type AuctionId = u32;

	fn new_collateral_auction(
		_initiator: &AccountId,
		_collateral_type: Self::CurrencyId,
		_amount: Self::Balance,
		_target: Self::Balance,
	) -> Result<(), DispatchError> {
		Ok(())
	}

	fn cancel_auction(_id: Self::AuctionId) -> DispatchResult { Ok(()) }

	fn get_total_collateral_in_auction(_collateral_type: Self::CurrencyId) -> Self::Balance {
		0
	}

	fn get_total_target_in_auction() -> Self::Balance {
		0
	}
}

pub struct MockSwap;
use pallet_traits::SwapLimit;
impl Swap<AccountId, Balance, CurrencyId> for MockSwap {
    fn swap(_source: &AccountId, _from: CurrencyId, _to: CurrencyId, _amount: SwapLimit<Balance>) -> Result<(Balance, Balance), DispatchError> { Ok((0, 0)) }
	fn get_swap_amount(_from: CurrencyId, _to: CurrencyId, _amount: SwapLimit<Balance>) -> Option<(Balance, Balance)> { Some((0, 0)) }
	fn swap_by_path(_source: &AccountId, _path: &[CurrencyId], _amount: SwapLimit<Balance>) -> Result<(Balance, Balance), DispatchError> { Ok((0,0)) }
	fn swap_by_aggregated_path<T, U>(
        _source: &AccountId,
        _path: &[AggregatedSwapPath<CurrencyId, T, U>],
        _amount: SwapLimit<Balance>,
    ) -> Result<(Balance, Balance), DispatchError> {
        let _ = PhantomData::<(T, U)>;
        Ok((0, 0))
    }
}

pub struct TreasuryAccount;
impl frame_support::traits::Get<AccountId> for TreasuryAccount {
	fn get() -> AccountId {
		ALICE
	}
}

impl pallet_cdp_treasury::Config for Test {
	type PalletId = CDPTreasuryPalletId;
    type Fungibles = MockFungibles;
    type AuctionManagerHandler = MockAuctionManager<AccountId>;
    type Swap = MockSwap;
    type UpdateOrigin = EnsureRoot<AccountId>;
    type WeightInfo = ();
    type Balance = Balance;
    type CurrencyId = CurrencyId;
    type MaxAuctionsCount = ConstU32<10>;
    type TreasuryAccount = TreasuryAccount;
						    type GetStableCurrencyId = StableCurrencyId;
    type GetBaseCurrencyId = CollateralCurrencyId;
}

pub struct MockPriceProvider;
impl PriceProvider<CurrencyId> for MockPriceProvider {
	fn get_relative_price(_base: CurrencyId, _quote: CurrencyId) -> Option<FixedU128> {
		Some(FixedU128::from_inner(100))
	}
    fn get_price(_currency_id: CurrencyId) -> Option<FixedU128> {
		Some(FixedU128::from_inner(100))
	}
}

parameter_types! {
	pub const CollateralCurrencyId: CurrencyId = NATIVE;
	pub const StableCurrencyId: CurrencyId = STABLE;
	pub const IssuanceBufferPalletId: PalletId = PalletId(*b"fr/issub");
    pub const CDPTreasuryPalletId: PalletId = PalletId(*b"fr/cdpty");
}

pub struct MockConvert;
impl ConversionToAssetBalance<Balance, CurrencyId, Balance> for MockConvert {
    type Error = DispatchError;
    fn to_asset_balance(balance: Balance, _asset_id: CurrencyId) -> result::Result<Balance, Self::Error> { Ok(balance) }
}

impl Config for Test {
	type AdminOrigin = EnsureRoot<AccountId>;
	type Currency = PalletBalances;
	type PriceProvider = MockPriceProvider;
	type CollateralCurrencyId = CollateralCurrencyId;
	type StableCurrencyId = StableCurrencyId;
	type PalletId = IssuanceBufferPalletId;
    type CDPTreasury = CDPTreasury;
}

pub struct ExtBuilder;

impl Default for ExtBuilder {
	fn default() -> Self {
		Self
	}
}

impl ExtBuilder {
	pub fn build(self) -> sp_io::TestExternalities {
		let t = frame_system::GenesisConfig::<Test>::default()
			.build_storage()
			.unwrap();



        let mut ext = sp_io::TestExternalities::new(t);
        ext.execute_with(|| System::set_block_number(1));
        ext
	}
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	ExtBuilder::default().build()
}
