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

//! Mocks for the auction manager module.

#![cfg(test)]

use super::{pallet, *};
use frame_support::{
	construct_runtime, defensive, derive_impl, ord_parameter_types, parameter_types,
	traits::{
		tokens::{
			fungibles, fungibles::Mutate, DepositConsequence, Fortitude, Precision, Preservation,
			Provenance, WithdrawConsequence,
		},
		AsEnsureOriginWithArg, ConstBool, ConstU128, ConstU32, ConstU64, Everything,
	},
	PalletId,
};
use frame_system::EnsureSignedBy;
use pallet_traits::{AggregatedSwapPath, EmergencyShutdown, PriceProvider, Rate, Swap, SwapLimit};
use sp_core::H256;
use sp_runtime::{
	traits::{AccountIdConversion, BlakeTwo256, IdentityLookup, One as OneT},
	BuildStorage, DispatchError,
};

pub type AccountId = u128;
pub type BlockNumber = u64;
pub type AuctionId = u32;
pub type Amount = i64;
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
		AuctionManagerModule: pallet,
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
	type Nonce = u64;
	type Block = Block;
	type BaseCallFilter = Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type BlockHashCount = ConstU64<250>;
	type Version = ();
	type PalletInfo = PalletInfo;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
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
	type GetStableCurrencyId = GetStableCurrencyId;
	type GetBaseCurrencyId = GetNativeCurrencyId;
	type Swap = MockSwap;
	type TreasuryAccount = TreasuryAccount;
}

pub struct MockCDPTreasury;
impl pallet_traits::CDPTreasury<AccountId> for MockCDPTreasury {
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
impl Swap<AccountId, Balance, CurrencyId> for MockSwap {
	fn get_swap_amount(
		_supply_currency_id: CurrencyId,
		_target_currency_id: CurrencyId,
		_limit: SwapLimit<Balance>,
	) -> Option<(Balance, Balance)> {
		None
	}

	fn swap(
		_who: &AccountId,
		_supply_currency_id: CurrencyId,
		_target_currency_id: CurrencyId,
		_limit: SwapLimit<Balance>,
	) -> Result<(Balance, Balance), DispatchError> {
		Err(DispatchError::Other("Not implemented"))
	}

	fn swap_by_path(
		_who: &AccountId,
		_swap_path: &[CurrencyId],
		_limit: SwapLimit<Balance>,
	) -> Result<(Balance, Balance), DispatchError> {
		Err(DispatchError::Other("Not implemented"))
	}

	fn swap_by_aggregated_path<StableAssetPoolId, PoolTokenIndex>(
		_who: &AccountId,
		_swap_path: &[AggregatedSwapPath<CurrencyId, StableAssetPoolId, PoolTokenIndex>],
		_limit: SwapLimit<Balance>,
	) -> Result<(Balance, Balance), DispatchError> {
		Err(DispatchError::Other("Not implemented"))
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

impl pallet::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeHoldReason = MockHoldReason;
	type Currency = MockCurrency;
	type Auction = AuctionModule;
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
	type Swap = MockSwap;
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

	pub fn lots_of_accounts() -> Self {
		let mut balances = Vec::new();
		for i in 0..1001 {
			let account_id: AccountId = i;
			balances.push((account_id, NATIVE, 1000));
		}
		Self { balances }
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
