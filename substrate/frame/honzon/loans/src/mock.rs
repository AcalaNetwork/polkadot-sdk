// This file is part of Substrate.

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

//! Mocks for the loans module.

use crate as pallet_loans;
use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::{
	construct_runtime, derive_impl, ord_parameter_types,
	pallet_prelude::*,
	parameter_types,
	traits::{
		honzon::{AuctionManager, DebitUnit, Handler, MockLiquidationStrategy, RiskManager},
		tokens::fungible::UnionOf,
		AsEnsureOriginWithArg, ConstU128, ConstU32,
	},
	PalletId,
};
use frame_system::{EnsureRoot, EnsureSignedBy};
use serde::{Deserialize, Serialize};
use sp_runtime::{
	traits::{AccountIdConversion, Convert, IdentityLookup, Zero},
	BuildStorage, DispatchError, DispatchResult, Either, FixedU128,
};
use std::collections::HashMap;

pub type AccountId = u128;
pub type Balance = u128;
pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;

construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		Loans: pallet_loans,
		Assets: pallet_assets,
		Balances: pallet_balances,
		CDPTreasuryModule: pallet_cdp_treasury,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = frame_system::mocking::MockBlock<Runtime>;
	type AccountData = pallet_balances::AccountData<Balance>;
}

impl pallet_balances::Config for Runtime {
	type Balance = Balance;
	type DustRemoval = ();
	type RuntimeEvent = RuntimeEvent;
	type ExistentialDeposit = ConstU128<1>;
	type AccountStore = frame_system::Pallet<Runtime>;
	type MaxLocks = ();
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type FreezeIdentifier = ();
	type WeightInfo = ();
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type MaxFreezes = ();
	type DoneSlashHandler = ();
}

pallet_assets::runtime_benchmarks_enabled! {
pub struct MockBenchmarkHelper;
impl pallet_assets::BenchmarkHelper<CurrencyId> for MockBenchmarkHelper {
	fn create_asset_id_parameter(id: u32) -> CurrencyId {
		CurrencyId::Asset(id)
	}
}
}

impl pallet_assets::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type AssetId = CurrencyId;
	type AssetIdParameter = CurrencyId;
	type Currency = Balances;
	type CreateOrigin = AsEnsureOriginWithArg<EnsureSignedBy<One, AccountId>>;
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
		type BenchmarkHelper = MockBenchmarkHelper;
	}
}

#[derive(
	Copy,
	Clone,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	Encode,
	Decode,
	RuntimeDebug,
	MaxEncodedLen,
	TypeInfo,
	Default,
	DecodeWithMemTracking,
	Serialize,
	Deserialize,
)]
pub enum CurrencyId {
	#[default]
	Native,
	Stable,
	Asset(u32),
}

pub struct MockAuctionManager;
impl AuctionManager<AccountId> for MockAuctionManager {
	type CurrencyId = CurrencyId;
	type Balance = Balance;
	type AuctionId = u32;
	fn new_collateral_auction(
		_refund_recipient: &AccountId,
		_currency_id: Self::CurrencyId,
		_amount: Balance,
		_target: Balance,
	) -> DispatchResult {
		Ok(())
	}

	fn cancel_auction(_id: u32) -> DispatchResult {
		Ok(())
	}

	fn get_total_target_in_auction() -> Balance {
		Default::default()
	}

	fn get_total_collateral_in_auction(_id: Self::CurrencyId) -> Balance {
		Default::default()
	}
}

ord_parameter_types! {
	pub const One: AccountId = 1;
}

parameter_types! {
	pub const CDPTreasuryPalletId: PalletId = PalletId(*b"py/cdptr");
	pub TreasuryAccount: AccountId = PalletId(*b"py/cdpta").into_account_truncating();
	pub const StableCurrencyId: CurrencyId = CurrencyId::Stable;
	pub const NativeCurrencyId: CurrencyId = CurrencyId::Native;
}

impl pallet_cdp_treasury::Config for Runtime {
	type CollateralCurrencyId = NativeCurrencyId;
	type StableCurrencyId = StableCurrencyId;
	type AuctionManagerHandler = MockAuctionManager;
	type Fungibles = LoansMultiCurrency;
	type UpdateOrigin = EnsureSignedBy<One, AccountId>;
	type MaxAuctionsCount = ConstU32<10_000>;
	type CurrencyId = CurrencyId;
	type PalletId = CDPTreasuryPalletId;
	type TreasuryAccount = TreasuryAccount;
	type WeightInfo = ();
	type Swap = MockSwap;
	type Balance = Balance;
	type AssetKind = CurrencyId;
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
/// mock risk manager
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
		pallet_loans::pallet::Positions::<Runtime>::iter().for_each(|(_who, position)| {
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

parameter_types! {
	pub static DotShares: HashMap<AccountId, Balance> = HashMap::new();
}

pub struct CurrencyIdConvert;
impl Convert<CurrencyId, Either<(), CurrencyId>> for CurrencyIdConvert {
	fn convert(currency_id: CurrencyId) -> Either<(), CurrencyId> {
		match currency_id {
			CurrencyId::Native => Either::Left(()),
			other => Either::Right(other),
		}
	}
}

type LoansMultiCurrency = UnionOf<Balances, Assets, CurrencyIdConvert, CurrencyId, AccountId>;

pub struct MockOnUpdateLoan;
impl Handler<(u128, crate::BalanceAdjustment<u128>, u128)> for MockOnUpdateLoan {
	fn handle(_info: &(u128, crate::BalanceAdjustment<u128>, u128)) -> DispatchResult {
		Ok(())
	}
}

parameter_types! {
	pub const LoansPalletId: PalletId = PalletId(*b"py/loans");
}

pub type NativeFungible = Balances;

impl pallet_loans::Config for Runtime {
	type Currency = NativeFungible;
	type RiskManager = MockRiskManager;
	type CDPTreasury = CDPTreasuryModule;
	type PalletId = LoansPalletId;
	type OnUpdateLoan = MockOnUpdateLoan;
	type LiquidationStrategy = MockLiquidationStrategy;
	type RuntimeHoldReason = RuntimeHoldReason;
}

pub struct ExtBuilder {
	balances: Vec<(AccountId, Balance)>,
	asset_balances: Vec<(AccountId, CurrencyId, Balance)>,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {
			balances: vec![(ALICE, 10000), (BOB, 10000)],
			asset_balances: vec![(ALICE, CurrencyId::Stable, 0), (BOB, CurrencyId::Stable, 0)],
		}
	}
}

impl ExtBuilder {
	pub fn build(self) -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

		pallet_balances::GenesisConfig::<Runtime> {
			balances: self.balances.iter().map(|(acc, b)| (*acc, *b)).collect(),
			dev_accounts: None,
		}
		.assimilate_storage(&mut t)
		.unwrap();

		pallet_assets::GenesisConfig::<Runtime> {
			assets: vec![(CurrencyId::Stable, ALICE, true, 1)],
			metadata: vec![(CurrencyId::Stable, b"Stable".to_vec(), b"STB".to_vec(), 12)],
			accounts: self
				.asset_balances
				.into_iter()
				.map(|(account, asset, balance)| (asset, account, balance))
				.collect(),
			next_asset_id: Some(CurrencyId::Asset(1)),
		}
		.assimilate_storage(&mut t)
		.unwrap();

		let mut ext = sp_io::TestExternalities::new(t);
		ext.execute_with(|| System::set_block_number(1));
		ext
	}
}
