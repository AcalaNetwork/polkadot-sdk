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
use frame_support::{
	construct_runtime, derive_impl, ord_parameter_types, parameter_types,
	traits::{ConstU128, ConstU32, Handler, Nothing},
	PalletId,
};
use frame_system::EnsureSignedBy;
use pallet_cdp_treasury::CDPTreasury;
use pallet_traits::{AuctionManager, RiskManager};
use sp_core::H256;
use sp_runtime::{
	testing::Header,
	traits::{AccountIdConversion, BlakeTwo256, IdentityLookup},
	BuildStorage, DispatchResult,
};
use std::collections::HashMap;

pub type AccountId = u128;
pub type Balance = u128;
pub type Amount = i128;
pub type BlockNumber = u64;

pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Runtime>;
type Block = frame_system::mocking::MockBlock<Runtime>;

construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		Loans: pallet,
		PalletBalances: pallet_balances,
		CDPTreasuryModule: pallet_cdp_treasury,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
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
	type WeightInfo = ();
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type FreezeIdentifier = ();
	type MaxFreezes = ();
}

#[derive(
	Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Encode, Decode, RuntimeDebug, MaxEncodedLen, TypeInfo,
)]
pub enum CurrencyId {
	Native,
	Stable,
}

pub struct MockAuctionManager;
impl AuctionManager<AccountId, Balance, u32> for MockAuctionManager {
	fn new_collateral_auction(
		_refund_recipient: &AccountId,
		_currency_id: u32,
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

	fn get_total_collateral_in_auction(_id: u32) -> Balance {
		Default::default()
	}
}

ord_parameter_types! {
	pub const One: AccountId = 1;
}

parameter_types! {
	pub const CDPTreasuryPalletId: PalletId = PalletId(*b"aca/cdpt");
	pub TreasuryAccount: AccountId = PalletId(*b"aca/hztr").into_account_truncating();
}

impl pallet_cdp_treasury::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = PalletBalances;
	type AuctionManagerHandler = MockAuctionManager;
	type UpdateOrigin = EnsureSignedBy<One, AccountId>;
	type DEX = ();
	type MaxAuctionsCount = ConstU32<10_000>;
	type PalletId = CDPTreasuryPalletId;
	type TreasuryAccount = TreasuryAccount;
	type WeightInfo = ();
	type CurrencyId = CurrencyId;
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

pub struct MockOnUpdateLoan;
impl Handler<(AccountId, Amount, Balance)> for MockOnUpdateLoan {
	fn handle(info: &(AccountId, Amount, Balance)) -> DispatchResult {
		let (who, adjustment, previous_amount) = info;
		let adjustment_abs = TryInto::<Balance>::try_into(adjustment.saturating_abs()).unwrap_or_default();
		let new_share_amount = if adjustment.is_positive() {
			previous_amount.saturating_add(adjustment_abs)
		} else {
			previous_amount.saturating_sub(adjustment_abs)
		};

		DotShares::mutate(|v| {
			let mut old_map = v.clone();
			old_map.insert(*who, new_share_amount);
			*v = old_map;
		});
		Ok(())
	}
}

parameter_types! {
	pub const LoansPalletId: PalletId = PalletId(*b"aca/loan");
	pub const CollateralCurrencyIdValue: CurrencyId = CurrencyId::Native;
}

impl Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = PalletBalances;
	type RiskManager = MockRiskManager;
	type CDPTreasury = CDPTreasuryModule;
	type PalletId = LoansPalletId;
		type OnUpdateLoan = MockOnUpdateLoan;
	type CurrencyId = CurrencyId;
	type CollateralCurrencyId = CollateralCurrencyIdValue;
}

pub struct ExtBuilder {
	balances: Vec<(AccountId, Balance)>,
}

impl Default for ExtBuilder {
	fn default() -> Self {
		Self {
			balances: vec![(ALICE, 10000), (BOB, 10000)],
		}
	}
}

impl ExtBuilder {
	pub fn build(self) -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::<Runtime>::default()
			.build_storage()
			.unwrap();
		pallet_balances::GenesisConfig::<Runtime> {
			balances: self.balances.iter().map(|(acc, b)| (*acc, *b)).collect(),
		}
		.assimilate_storage(&mut t)
		.unwrap();

		let mut ext = sp_io::TestExternalities::new(t);
		ext.execute_with(|| System::set_block_number(1));
		ext
	}
}
