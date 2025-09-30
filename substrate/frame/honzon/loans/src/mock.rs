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

//! Mocks for the loans module.

#![cfg(test)]

use super::*;
use codec::{Decode, Encode};
use frame_support::{
	construct_runtime, derive_impl, ord_parameter_types,
	pallet_prelude::*,
	parameter_types,
	traits::{tokens::fungible::UnionOf, AsEnsureOriginWithArg, ConstU128, ConstU32},
	PalletId,
};
use frame_system::{EnsureRoot, EnsureSignedBy};
use pallet_traits::{honzon::*, Handler, MockLiquidationStrategy, Swap, SwapLimit};
use sp_runtime::{
	traits::{AccountIdConversion, Convert, IdentityLookup},
	BuildStorage, DispatchResult, Either,
};
use std::collections::HashMap;

pub type AccountId = u128;
pub type Balance = u128;
pub type Amount = i128;
pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;

construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		Loans: pallet,
		Assets: pallet_assets,
		PalletBalances: pallet_balances,
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

impl pallet_assets::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type AssetId = CurrencyId;
	type AssetIdParameter = CurrencyId;
	type Currency = PalletBalances;
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
	codec::DecodeWithMemTracking,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
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
	pub const CDPTreasuryPalletId: PalletId = PalletId(*b"aca/cdpt");
	pub TreasuryAccount: AccountId = PalletId(*b"aca/hztr").into_account_truncating();
	pub const StableCurrencyIdValue: CurrencyId = CurrencyId::Stable;
}

pub struct MockSwap;
impl Swap<u128, u128, CurrencyId> for MockSwap {
	fn swap(
		_who: &u128,
		_from: CurrencyId,
		_to: CurrencyId,
		_limit: SwapLimit<u128>,
	) -> Result<(u128, u128), DispatchError> {
		Ok((1, 1))
	}

	fn get_swap_amount(
		_from: CurrencyId,
		_to: CurrencyId,
		_limit: SwapLimit<u128>,
	) -> Option<(u128, u128)> {
		Some((1, 1))
	}

	fn swap_by_path(
		_who: &u128,
		_path: &[CurrencyId],
		_limit: SwapLimit<u128>,
	) -> Result<(u128, u128), DispatchError> {
		Ok((1, 1))
	}

	fn swap_by_aggregated_path<StableAssetPoolId, PoolTokenIndex>(
		_who: &u128,
		_path: &[pallet_traits::AggregatedSwapPath<
			CurrencyId,
			StableAssetPoolId,
			PoolTokenIndex,
		>],
		_limit: SwapLimit<u128>,
	) -> Result<(u128, u128), DispatchError> {
		Ok((1, 1))
	}
}

impl pallet_cdp_treasury::Config for Runtime {
	type Fungibles = LoansMultiCurrency;
	type AuctionManagerHandler = MockAuctionManager;
	type UpdateOrigin = EnsureSignedBy<One, AccountId>;
	type MaxAuctionsCount = ConstU32<10_000>;
	type PalletId = CDPTreasuryPalletId;
	type TreasuryAccount = TreasuryAccount;
	type WeightInfo = ();
	type CurrencyId = CurrencyId;
	type GetStableCurrencyId = StableCurrencyIdValue;
	type GetBaseCurrencyId = CollateralCurrencyIdValue;
	type Swap = MockSwap;
	type Balance = Balance;
}

// mock risk manager
pub struct MockRiskManager;
impl RiskManager<AccountId, CurrencyId, Balance, Balance> for MockRiskManager {
	fn get_debit_value(_currency_id: CurrencyId, debit_balance: Balance) -> Balance {
		debit_balance / 2
	}

	fn check_position_valid(
		_currency_id: CurrencyId,
		collateral_balance: Balance,
		debit_balance: Balance,
		check_required_ratio: bool,
	) -> DispatchResult {
		if debit_balance > 0 && check_required_ratio {
			if collateral_balance < debit_balance * 2 {
				return Err(sp_runtime::DispatchError::Other(
					"mock below required collateral ratio error",
				));
			}
		}
		if collateral_balance < debit_balance {
			return Err(sp_runtime::DispatchError::Other("mock below liquidation ratio error"));
		}
		Ok(())
	}

	fn check_debit_cap(_currency_id: CurrencyId, total_debit_balance: Balance) -> DispatchResult {
		if total_debit_balance > 1000 {
			Err(sp_runtime::DispatchError::Other("mock exceed debit value cap error"))
		} else {
			Ok(())
		}
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

type LoansMultiCurrency = UnionOf<Collateral, Assets, CurrencyIdConvert, CurrencyId, AccountId>;

pub struct MockOnUpdateLoan;
impl Handler<(u128, i128, u128)> for MockOnUpdateLoan {
	fn handle(_info: &(u128, i128, u128)) -> DispatchResult {
		Ok(())
	}
}

parameter_types! {
	pub const LoansPalletId: PalletId = PalletId(*b"aca/loan");
	pub const CollateralCurrencyIdValue: CurrencyId = CurrencyId::Native;
}

pub type Collateral = PalletBalances;

impl Config for Runtime {
	type Amount = Amount;
	type RiskManager = MockRiskManager;
	type CDPTreasury = CDPTreasuryModule;
	type PalletId = LoansPalletId;
	type OnUpdateLoan = MockOnUpdateLoan;
	type CurrencyId = CurrencyId;
	type CollateralCurrencyId = CollateralCurrencyIdValue;
	type LiquidationStrategy = MockLiquidationStrategy;
	type RuntimeHoldReason = RuntimeHoldReason;
	type Currency = Collateral;
}

pub struct ExtBuilder {
	balances: Vec<(AccountId, CurrencyId, Balance)>,
	asset_balances: Vec<(AccountId, Balance)>,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {
			balances: vec![(ALICE, CurrencyId::Stable, 10000), (BOB, CurrencyId::Stable, 10000)],
			asset_balances: vec![(ALICE, 10000), (BOB, 10000)],
		}
	}
}

impl ExtBuilder {
	pub fn build(self) -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

		pallet_balances::GenesisConfig::<Runtime> {
			balances: self.asset_balances.iter().map(|(acc, b)| (*acc, *b)).collect(),
			dev_accounts: None,
		}
		.assimilate_storage(&mut t)
		.unwrap();

		pallet_assets::GenesisConfig::<Runtime> {
			assets: vec![(CurrencyId::Stable, ALICE, true, 1)],
			metadata: vec![(CurrencyId::Stable, b"Stable".to_vec(), b"STB".to_vec(), 12)],
			accounts: self
				.balances
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
