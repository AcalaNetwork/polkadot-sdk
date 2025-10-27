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

//! Mocks for the auction manager module.

#![cfg(test)]

use crate as pallet_auction_manager;
use frame_support::{
	construct_runtime, derive_impl, ord_parameter_types,
	pallet_prelude::*,
	parameter_types,
	traits::{
		honzon::{CDPTreasury, EmergencyShutdown, PriceProvider, Rate},
		tokens::{
			fungibles, DepositConsequence, Fortitude, Precision, Preservation, Provenance,
			WithdrawConsequence,
		},
		AsEnsureOriginWithArg, ConstU128, ConstU64,
	},
	PalletId,
};
use frame_system::EnsureSignedBy;
use sp_core::H256;
use sp_runtime::{
	traits::{AccountIdConversion, BlakeTwo256, IdentityLookup, One as OneT},
	BuildStorage, FixedPointNumber,
};

pub type AccountId = u128;
pub type BlockNumber = u64;
pub type AuctionId = u32;
pub type Balance = u128;

pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;
pub const CAROL: AccountId = 3;

#[derive(
	Encode,
	Decode,
	Clone,
	Eq,
	PartialEq,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
	PartialOrd,
	Ord,
	Copy,
	codec::DecodeWithMemTracking,
)]
pub enum MockHoldReason {
	Hold,
}

impl frame_support::traits::VariantCount for MockHoldReason {
	const VARIANT_COUNT: u32 = 1;
}

impl From<crate::HoldReason> for MockHoldReason {
	fn from(_: crate::HoldReason) -> Self {
		MockHoldReason::Hold
	}
}

#[derive(
	Encode,
	Decode,
	Eq,
	PartialEq,
	Copy,
	Clone,
	RuntimeDebug,
	PartialOrd,
	Ord,
	TypeInfo,
	MaxEncodedLen,
	codec::DecodeWithMemTracking,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum CurrencyId {
	/// Native currency
	Native,
	/// Stable currency
	Stable,
}

pub const NATIVE: CurrencyId = CurrencyId::Native;
pub const STABLE: CurrencyId = CurrencyId::Stable;

type Block = frame_system::mocking::MockBlock<Runtime>;

construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		AuctionManagerModule: pallet_auction_manager,
		Assets: pallet_assets,
		AuctionModule: pallet_auction,
		CDPTreasuryModule: pallet_cdp_treasury,
		Balances: pallet_balances,
		AssetsFreezer: pallet_assets_freezer,
		AssetsHolder: pallet_assets_holder,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type AccountData = pallet_balances::AccountData<Balance>;
	type Block = Block;
	type Nonce = u64;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type PalletInfo = PalletInfo;
	type MaxConsumers = ConstU32<16>;
	type RuntimeEvent = RuntimeEvent;
}

impl pallet_balances::Config for Runtime {
	type Balance = Balance;
	type DustRemoval = ();
	type RuntimeEvent = RuntimeEvent;
	type ExistentialDeposit = ConstU128<1>;
	type AccountStore = System;
	type WeightInfo = ();
	type MaxLocks = ();
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type RuntimeHoldReason = MockHoldReason;
	type RuntimeFreezeReason = ();
	type FreezeIdentifier = ();
	type MaxFreezes = ();
	type DoneSlashHandler = ();
}

impl pallet_assets_freezer::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeFreezeReason = MockHoldReason;
}

impl pallet_assets_holder::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeHoldReason = MockHoldReason;
}

pallet_assets::runtime_benchmarks_enabled! {
pub struct MockBenchmarkHelper;
// TODO: Check if this makes sense
impl pallet_assets::BenchmarkHelper<CurrencyId> for MockBenchmarkHelper {
	fn create_asset_id_parameter(id: u32) -> CurrencyId {
		if id == 0 {
			CurrencyId::Native
		} else {
			CurrencyId::Stable
		}
	}
}
}

impl pallet_assets::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type AssetId = CurrencyId;
	type AssetIdParameter = CurrencyId;
	type Currency = Balances;
	type CreateOrigin = AsEnsureOriginWithArg<frame_system::EnsureSigned<AccountId>>;
	type ForceOrigin = frame_system::EnsureRoot<AccountId>;
	type AssetDeposit = ConstU128<1>;
	type AssetAccountDeposit = ConstU128<10>;
	type MetadataDepositBase = ConstU128<1>;
	type MetadataDepositPerByte = ConstU128<1>;
	type ApprovalDeposit = ConstU128<1>;
	type StringLimit = ConstU32<50>;
	type Freezer = AssetsFreezer;
	type Extra = ();
	type WeightInfo = ();
	type RemoveItemsLimit = ConstU32<1000>;
	type CallbackHandle = ();
	type Holder = AssetsHolder;
	pallet_assets::runtime_benchmarks_enabled! {
		type BenchmarkHelper = MockBenchmarkHelper;
	}
}

impl pallet_auction::Config for Runtime {
	type AuctionId = AuctionId;
	type Handler = AuctionManagerModule;
	type WeightInfo = ();
	type Balance = Balance;
}

ord_parameter_types! {
	pub const One: AccountId = 1;
}

parameter_types! {
	pub const GetStableCurrencyId: CurrencyId = STABLE;
	pub const MaxAuctionsCount: u32 = 10_000;
	pub const CDPTreasuryPalletId: PalletId = PalletId(*b"aca/cdpt");
	pub TreasuryAccount: AccountId = PalletId(*b"aca/hztr").into_account_truncating();
}

impl pallet_cdp_treasury::Config for Runtime {
	type UpdateOrigin = EnsureSignedBy<One, AccountId>;
	type Fungibles = Assets;
	type AuctionManagerHandler = AuctionManagerModule;
	type Balance = Balance;
	type CurrencyId = CurrencyId;
	type MaxAuctionsCount = MaxAuctionsCount;
	type PalletId = CDPTreasuryPalletId;
	type WeightInfo = ();
	type StableCurrencyId = GetStableCurrencyId;
	type CollateralCurrencyId = GetNativeCurrencyId;
	type AssetKind = CurrencyId;
	type Swap = MockSwap;
	type TreasuryAccount = TreasuryAccount;
}

pub struct MockCDPTreasury;
impl CDPTreasury<AccountId> for MockCDPTreasury {
	fn account_id() -> AccountId {
		TreasuryAccount::get()
	}
	type Balance = Balance;
	type CurrencyId = CurrencyId;

	fn get_surplus_pool() -> Self::Balance {
		Default::default()
	}
	fn get_debit_pool() -> Self::Balance {
		Default::default()
	}
	fn get_total_collaterals() -> Self::Balance {
		Default::default()
	}
	fn get_debit_proportion(_amount: Self::Balance) -> Rate {
		Default::default()
	}
	fn on_system_debit(_amount: Self::Balance) -> DispatchResult {
		Ok(())
	}
	fn on_system_surplus(_amount: Self::Balance) -> DispatchResult {
		Ok(())
	}
	fn issue_debit(_who: &AccountId, _debit: Self::Balance, _backed: bool) -> DispatchResult {
		Ok(())
	}
	fn burn_debit(_who: &AccountId, _debit: Self::Balance) -> DispatchResult {
		Ok(())
	}
	fn deposit_surplus(_from: &AccountId, _surplus: Self::Balance) -> DispatchResult {
		Ok(())
	}
	fn withdraw_surplus(_to: &AccountId, _surplus: Self::Balance) -> DispatchResult {
		Ok(())
	}
	fn deposit_collateral(_from: &AccountId, _amount: Self::Balance) -> DispatchResult {
		Ok(())
	}
	fn withdraw_collateral(_to: &AccountId, _amount: Self::Balance) -> DispatchResult {
		Ok(())
	}
	fn pay_surplus(_amount: Self::Balance) -> DispatchResult {
		Ok(())
	}
	fn refund_surplus(_amount: Self::Balance) -> DispatchResult {
		Ok(())
	}
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
	static RelativePrice: Option<Rate> = Some(Rate::one());
}

pub struct MockPriceSource;
impl MockPriceSource {
	pub fn set_relative_price(price: Option<Rate>) {
		RelativePrice::mutate(|v| *v = price);
	}
}
impl PriceProvider<CurrencyId> for MockPriceSource {
	fn get_price(_currency_id: CurrencyId) -> Option<Rate> {
		Some(Rate::one())
	}
	fn get_relative_price(base: CurrencyId, quote: CurrencyId) -> Option<Rate> {
		if base == NATIVE && quote == STABLE {
			RelativePrice::get()
		} else if base == STABLE && quote == NATIVE {
			Some(Rate::one() / RelativePrice::get().unwrap())
		} else {
			None
		}
	}
}

parameter_types! {
	pub const GetNativeCurrencyId: CurrencyId = NATIVE;
}

parameter_types! {
	static IsShutdown: bool = false;
}

pub fn mock_shutdown() {
	IsShutdown::mutate(|v| *v = true)
}

pub struct MockEmergencyShutdown;
impl EmergencyShutdown for MockEmergencyShutdown {
	fn is_shutdown() -> bool {
		IsShutdown::get()
	}
}

parameter_types! {
	pub MinimumIncrementSize: Rate = Rate::saturating_from_rational(1, 20);
}

pub struct MockCurrency;

impl pallet_auction_manager::Config for Runtime {
	type RuntimeHoldReason = MockHoldReason;
	type Fungibles = MockCurrency;
	type MinimumIncrementSize = MinimumIncrementSize;
	type AuctionTimeToClose = ConstU64<100>;
	type AuctionDurationSoftCap = ConstU64<2000>;
	type GetStableCurrencyId = GetStableCurrencyId;
	type GetNativeCurrencyId = GetNativeCurrencyId;
	type CDPTreasury = CDPTreasuryModule;
	type PriceSource = MockPriceSource;
	type EmergencyShutdown = MockEmergencyShutdown;
	type WeightInfo = ();
	type CurrencyId = CurrencyId;
}

pub struct ExtBuilder {
	balances: Vec<(AccountId, CurrencyId, Balance)>,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {
			balances: vec![
				(ALICE, STABLE, 1000),
				(BOB, STABLE, 1000),
				(CAROL, STABLE, 1000),
				(ALICE, NATIVE, 1000),
				(BOB, NATIVE, 1000),
				(CAROL, NATIVE, 1000),
			],
		}
	}
}

impl ExtBuilder {
	pub fn with_treasury_collateral(mut self, amount: Balance) -> Self {
		self.balances
			.push((CDPTreasuryPalletId::get().into_account_truncating(), NATIVE, amount));
		self
	}

	pub fn build(self) -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

		let mut accounts = self.balances.iter().map(|(id, _, _)| *id).collect::<Vec<_>>();
		accounts.sort();
		accounts.dedup();

		pallet_assets::GenesisConfig::<Runtime> {
			assets: vec![(NATIVE, accounts[0], true, 1), (STABLE, accounts[0], true, 1)],
			accounts: self
				.balances
				.into_iter()
				.map(|(id, asset, balance)| (asset, id, balance))
				.collect(),
			..Default::default()
		}
		.assimilate_storage(&mut t)
		.unwrap();

		t.into()
	}
}

impl fungibles::Inspect<AccountId> for MockCurrency {
	type AssetId = CurrencyId;
	type Balance = Balance;

	fn total_issuance(asset: Self::AssetId) -> Self::Balance {
		pallet_assets::Pallet::<Runtime>::total_issuance(asset)
	}

	fn minimum_balance(asset: Self::AssetId) -> Self::Balance {
		pallet_assets::Pallet::<Runtime>::minimum_balance(asset)
	}

	fn balance(asset: Self::AssetId, who: &AccountId) -> Self::Balance {
		pallet_assets::Pallet::<Runtime>::balance(asset, who)
	}

	fn total_balance(asset: Self::AssetId, who: &AccountId) -> Self::Balance {
		pallet_assets::Pallet::<Runtime>::total_balance(asset, who)
	}

	fn reducible_balance(
		asset: Self::AssetId,
		who: &AccountId,
		preservation: Preservation,
		fortitude: Fortitude,
	) -> Self::Balance {
		pallet_assets::Pallet::<Runtime>::reducible_balance(asset, who, preservation, fortitude)
	}

	fn can_deposit(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
		provenance: Provenance,
	) -> DepositConsequence {
		pallet_assets::Pallet::<Runtime>::can_deposit(asset, who, amount, provenance)
	}

	fn can_withdraw(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
	) -> WithdrawConsequence<Self::Balance> {
		pallet_assets::Pallet::<Runtime>::can_withdraw(asset, who, amount)
	}

	fn asset_exists(asset: Self::AssetId) -> bool {
		pallet_assets::Pallet::<Runtime>::asset_exists(asset)
	}
}

impl fungibles::Mutate<AccountId> for MockCurrency {
	fn done_mint_into(asset_id: Self::AssetId, beneficiary: &AccountId, amount: Self::Balance) {
		pallet_assets::Pallet::<Runtime>::done_mint_into(asset_id, beneficiary, amount)
	}

	fn done_burn_from(asset_id: Self::AssetId, target: &AccountId, balance: Self::Balance) {
		pallet_assets::Pallet::<Runtime>::done_burn_from(asset_id, target, balance)
	}

	fn done_transfer(
		asset_id: Self::AssetId,
		source: &AccountId,
		dest: &AccountId,
		amount: Self::Balance,
	) {
		pallet_assets::Pallet::<Runtime>::done_transfer(asset_id, source, dest, amount)
	}
}

impl fungibles::Balanced<AccountId> for MockCurrency {
	type OnDropCredit =
		<pallet_assets::Pallet<Runtime> as fungibles::Balanced<AccountId>>::OnDropCredit;
	type OnDropDebt =
		<pallet_assets::Pallet<Runtime> as fungibles::Balanced<AccountId>>::OnDropDebt;

	fn done_deposit(asset_id: Self::AssetId, who: &AccountId, amount: Self::Balance) {
		pallet_assets::Pallet::<Runtime>::done_deposit(asset_id, who, amount)
	}

	fn done_withdraw(asset_id: Self::AssetId, who: &AccountId, amount: Self::Balance) {
		pallet_assets::Pallet::<Runtime>::done_withdraw(asset_id, who, amount)
	}
}

impl fungibles::hold::Inspect<AccountId> for MockCurrency {
	type Reason = MockHoldReason;

	fn total_balance_on_hold(asset: Self::AssetId, who: &AccountId) -> Self::Balance {
		pallet_assets_holder::Pallet::<Runtime>::total_balance_on_hold(asset, who)
	}

	fn balance_on_hold(
		asset: Self::AssetId,
		reason: &Self::Reason,
		who: &AccountId,
	) -> Self::Balance {
		pallet_assets_holder::Pallet::<Runtime>::balance_on_hold(asset, reason, who)
	}
}

impl fungibles::hold::Mutate<AccountId> for MockCurrency {
	fn done_hold(
		asset_id: Self::AssetId,
		reason: &Self::Reason,
		who: &AccountId,
		amount: Self::Balance,
	) {
		pallet_assets_holder::Pallet::<Runtime>::done_hold(asset_id, reason, who, amount)
	}

	fn done_release(
		asset_id: Self::AssetId,
		reason: &Self::Reason,
		who: &AccountId,
		amount: Self::Balance,
	) {
		pallet_assets_holder::Pallet::<Runtime>::done_release(asset_id, reason, who, amount)
	}

	fn done_burn_held(
		asset_id: Self::AssetId,
		reason: &Self::Reason,
		who: &AccountId,
		amount: Self::Balance,
	) {
		pallet_assets_holder::Pallet::<Runtime>::done_burn_held(asset_id, reason, who, amount)
	}
}

impl fungibles::Unbalanced<AccountId> for MockCurrency {
	fn handle_raw_dust(_: Self::AssetId, _: Self::Balance) {
		defensive!("`decrease_balance` and `increase_balance` have non-default impls; nothing else calls this; qed");
	}
	fn handle_dust(_: fungibles::Dust<AccountId, Self>) {
		defensive!("`decrease_balance` and `increase_balance` have non-default impls; nothing else calls this; qed");
	}
	fn write_balance(
		asset: Self::AssetId,
		who: &AccountId,
		balance: Self::Balance,
	) -> Result<Option<Self::Balance>, DispatchError> {
		pallet_assets::Pallet::<Runtime>::write_balance(asset, who, balance)
	}
	fn set_total_issuance(asset: Self::AssetId, amount: Self::Balance) {
		pallet_assets::Pallet::<Runtime>::set_total_issuance(asset, amount)
	}
	fn decrease_balance(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
		precision: Precision,
		preservation: Preservation,
		fortitude: Fortitude,
	) -> Result<Self::Balance, DispatchError> {
		pallet_assets::Pallet::<Runtime>::decrease_balance(
			asset,
			who,
			amount,
			precision,
			preservation,
			fortitude,
		)
	}
	fn increase_balance(
		asset: Self::AssetId,
		who: &AccountId,
		amount: Self::Balance,
		precision: Precision,
	) -> Result<Self::Balance, DispatchError> {
		pallet_assets::Pallet::<Runtime>::increase_balance(asset, who, amount, precision)
	}
}

impl fungibles::hold::Unbalanced<AccountId> for MockCurrency {
	fn set_balance_on_hold(
		asset: Self::AssetId,
		reason: &Self::Reason,
		who: &AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		pallet_assets_holder::Pallet::<Runtime>::set_balance_on_hold(asset, reason, who, amount)
	}
}
