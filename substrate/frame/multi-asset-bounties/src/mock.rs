// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! bounties pallet tests.

#![cfg(test)]

use crate as pallet_bounties;
use crate::{Event as BountiesEvent, *};

use alloc::collections::btree_map::BTreeMap;
use core::cell::RefCell;
use frame_support::{
	assert_ok, derive_impl, parameter_types,
	traits::{
		tokens::{Pay, UnityAssetBalanceConversion},
		ConstU32, ConstU64, Currency, OnInitialize,
	},
	weights::constants::ParityDbWeight,
	PalletId,
};
use sp_runtime::{traits::IdentityLookup, BuildStorage, Perbill};

type Block = frame_system::mocking::MockBlock<Test>;

thread_local! {
	pub static PAID: RefCell<BTreeMap<(u128, u32), u64>> = RefCell::new(BTreeMap::new());
	pub static STATUS: RefCell<BTreeMap<u64, PaymentStatus>> = RefCell::new(BTreeMap::new());
	pub static LAST_ID: RefCell<u64> = RefCell::new(0u64);

	#[cfg(feature = "runtime-benchmarks")]
	pub static TEST_SPEND_ORIGIN_TRY_SUCCESSFUL_ORIGIN_ERR: RefCell<bool> = RefCell::new(false);
}

pub struct TestTreasuryPay;
impl Pay for TestTreasuryPay {
	type Beneficiary = u128;
	type Balance = u64;
	type Id = u64;
	type AssetKind = u32;
	type Error = ();

	fn pay(
		_: &Self::Beneficiary,
		_: Self::AssetKind,
		_: Self::Balance,
	) -> Result<Self::Id, Self::Error> {
		Ok(0)
	}
	fn check_payment(_: Self::Id) -> PaymentStatus {
		PaymentStatus::InProgress
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(_: &Self::Beneficiary, _: Self::AssetKind, _: Self::Balance) {}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_concluded(_: Self::Id) {}
}

pub struct TestBountiesPay;
impl PayWithSource for TestBountiesPay {
	type Source = u128;
	type Beneficiary = u128;
	type Balance = u64;
	type Id = u64;
	type AssetKind = u32;
	type Error = ();

	fn pay(
		_: &Self::Source,
		to: &Self::Beneficiary,
		asset_kind: Self::AssetKind,
		amount: Self::Balance,
	) -> Result<Self::Id, Self::Error> {
		PAID.with(|paid| *paid.borrow_mut().entry((*to, asset_kind)).or_default() += amount);
		Ok(LAST_ID.with(|lid| {
			let x = *lid.borrow();
			lid.replace(x + 1);
			x
		}))
	}
	fn check_payment(id: Self::Id) -> PaymentStatus {
		STATUS.with(|s| s.borrow().get(&id).cloned().unwrap_or(PaymentStatus::InProgress))
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_successful(
		_: &Self::Source,
		_: &Self::Beneficiary,
		_: Self::AssetKind,
		_: Self::Balance,
	) {
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn ensure_concluded(id: Self::Id) {
		set_status(id, PaymentStatus::Success);
	}
}

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		Utility: pallet_utility,
		Bounties: pallet_bounties,
		Bounties1: pallet_bounties::<Instance1>,
		Treasury: pallet_treasury,
		Treasury1: pallet_treasury::<Instance1>,
	}
);

parameter_types! {
	pub const AvailableBlockRatio: Perbill = Perbill::one();
}

type Balance = u64;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type AccountId = u128; // u64 is not enough to hold bytes used to generate bounty account
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u64>;
	type DbWeight = ParityDbWeight;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

impl pallet_utility::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type PalletsOrigin = OriginCaller;
	type WeightInfo = ();
}

parameter_types! {
	pub static Burn: Permill = Permill::from_percent(50);
	pub const TreasuryPalletId: PalletId = PalletId(*b"py/trsry");
	pub const TreasuryPalletId2: PalletId = PalletId(*b"py/trsr2");
	pub static SpendLimit: Balance = u64::MAX;
	pub static SpendLimit1: Balance = u64::MAX;
	pub TreasuryAccount: u128 = Treasury::account_id();
	pub TreasuryInstance1Account: u128 = Treasury1::account_id();
}

pub struct TestSpendOrigin;
impl frame_support::traits::EnsureOrigin<RuntimeOrigin> for TestSpendOrigin {
	type Success = u64;
	fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
		Result::<frame_system::RawOrigin<_>, RuntimeOrigin>::from(o).and_then(|o| match o {
			frame_system::RawOrigin::Root => Ok(SpendLimit::get()),
			frame_system::RawOrigin::Signed(10) => Ok(5),
			frame_system::RawOrigin::Signed(11) => Ok(10),
			frame_system::RawOrigin::Signed(12) => Ok(20),
			frame_system::RawOrigin::Signed(13) => Ok(50),
			frame_system::RawOrigin::Signed(14) => Ok(500),
			r => Err(RuntimeOrigin::from(r)),
		})
	}
	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
		if TEST_SPEND_ORIGIN_TRY_SUCCESSFUL_ORIGIN_ERR.with(|i| *i.borrow()) {
			Err(())
		} else {
			Ok(frame_system::RawOrigin::Root.into())
		}
	}
}

impl pallet_treasury::Config for Test {
	type PalletId = TreasuryPalletId;
	type Currency = pallet_balances::Pallet<Test>;
	type RejectOrigin = frame_system::EnsureRoot<u128>;
	type RuntimeEvent = RuntimeEvent;
	type SpendPeriod = ConstU64<2>;
	type Burn = Burn;
	type BurnDestination = (); // Just gets burned.
	type WeightInfo = ();
	type SpendFunds = ();
	type MaxApprovals = ConstU32<100>;
	type SpendOrigin = TestSpendOrigin;
	type AssetKind = u32;
	type Beneficiary = Self::AccountId;
	type BeneficiaryLookup = IdentityLookup<Self::Beneficiary>;
	type Paymaster = TestTreasuryPay;
	type BalanceConverter = UnityAssetBalanceConversion;
	type PayoutPeriod = ConstU64<10>;
	type BlockNumberProvider = System;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

impl pallet_treasury::Config<Instance1> for Test {
	type PalletId = TreasuryPalletId2;
	type Currency = pallet_balances::Pallet<Test>;
	type RejectOrigin = frame_system::EnsureRoot<u128>;
	type RuntimeEvent = RuntimeEvent;
	type SpendPeriod = ConstU64<2>;
	type Burn = Burn;
	type BurnDestination = (); // Just gets burned.
	type WeightInfo = ();
	type SpendFunds = ();
	type MaxApprovals = ConstU32<100>;
	type SpendOrigin = frame_system::EnsureRootWithSuccess<Self::AccountId, SpendLimit1>;
	type AssetKind = u32;
	type Beneficiary = Self::AccountId;
	type BeneficiaryLookup = IdentityLookup<Self::Beneficiary>;
	type Paymaster = TestTreasuryPay;
	type BalanceConverter = UnityAssetBalanceConversion;
	type PayoutPeriod = ConstU64<10>;
	type BlockNumberProvider = System;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

parameter_types! {
	// This will be 50% of the bounty fee.
	pub const CuratorDepositMultiplier: Permill = Permill::from_percent(50);
	pub const CuratorDepositMax: Balance = 1_000;
	pub const CuratorDepositMin: Balance = 3;
	pub static MaxActiveChildBountyCount: u32 = 3;
}

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type CuratorDepositMultiplier = CuratorDepositMultiplier;
	type CuratorDepositMax = CuratorDepositMax;
	type CuratorDepositMin = CuratorDepositMin;
	type BountyValueMinimum = ConstU64<2>;
	type ChildBountyValueMinimum = ConstU64<1>;
	type MaxActiveChildBountyCount = MaxActiveChildBountyCount;
	type DataDepositPerByte = ConstU64<1>;
	type MaximumReasonLength = ConstU32<16384>;
	type WeightInfo = ();
	type OnSlash = ();
	type TreasurySource = TreasurySource<Test, ()>;
	type BountySource = BountySource<Test, ()>;
	type ChildBountySource = ChildBountySource<Test, ()>;
	type Paymaster = TestBountiesPay;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

impl Config<Instance1> for Test {
	type RuntimeEvent = RuntimeEvent;
	type CuratorDepositMultiplier = CuratorDepositMultiplier;
	type CuratorDepositMax = CuratorDepositMax;
	type CuratorDepositMin = CuratorDepositMin;
	type BountyValueMinimum = ConstU64<2>;
	type ChildBountyValueMinimum = ConstU64<1>;
	type MaxActiveChildBountyCount = MaxActiveChildBountyCount;
	type DataDepositPerByte = ConstU64<1>;
	type MaximumReasonLength = ConstU32<16384>;
	type WeightInfo = ();
	type OnSlash = ();
	type TreasurySource = TreasurySource<Test, Instance1>;
	type BountySource = BountySource<Test, Instance1>;
	type ChildBountySource = ChildBountySource<Test, Instance1>;
	type Paymaster = TestBountiesPay;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

pub struct ExtBuilder {}

impl Default for ExtBuilder {
	fn default() -> Self {
		#[cfg(feature = "runtime-benchmarks")]
		TEST_SPEND_ORIGIN_TRY_SUCCESSFUL_ORIGIN_ERR.with(|i| *i.borrow_mut() = false);
		Self {}
	}
}

impl ExtBuilder {
	#[cfg(feature = "runtime-benchmarks")]
	pub fn spend_origin_succesful_origin_err(self) -> Self {
		TEST_SPEND_ORIGIN_TRY_SUCCESSFUL_ORIGIN_ERR.with(|i| *i.borrow_mut() = true);
		self
	}

	pub fn build(self) -> sp_io::TestExternalities {
		let mut ext: sp_io::TestExternalities = RuntimeGenesisConfig {
			system: frame_system::GenesisConfig::default(),
			balances: pallet_balances::GenesisConfig {
				balances: vec![(0, 100), (1, 98), (2, 1)],
				..Default::default()
			},
			treasury: Default::default(),
			treasury_1: Default::default(),
		}
		.build_storage()
		.unwrap()
		.into();
		ext.execute_with(|| {
			<Test as pallet_treasury::Config>::BlockNumberProvider::set_block_number(1);
		});
		ext
	}

	pub fn build_and_execute(self, test: impl FnOnce() -> ()) {
		self.build().execute_with(|| {
			test();
			Bounties::do_try_state().expect("All invariants must hold after a test");
			Bounties1::do_try_state().expect("All invariants must hold after a test");
		})
	}
}

// This function directly jumps to a block number, and calls `on_initialize`.
pub fn go_to_block(n: u64) {
	<Test as pallet_treasury::Config>::BlockNumberProvider::set_block_number(n);
	<Treasury as OnInitialize<u64>>::on_initialize(n);
}

/// paid balance for a given account and asset ids
pub fn paid(who: u128, asset_id: u32) -> u64 {
	PAID.with(|p| p.borrow().get(&(who, asset_id)).cloned().unwrap_or(0))
}

/// reduce paid balance for a given account and asset ids
pub fn unpay(who: u128, asset_id: u32, amount: u64) {
	PAID.with(|p| p.borrow_mut().entry((who, asset_id)).or_default().saturating_reduce(amount))
}

/// set status for a given payment id
pub fn set_status(id: u64, s: PaymentStatus) {
	STATUS.with(|m| m.borrow_mut().insert(id, s));
}

pub fn last_events(n: usize) -> Vec<BountiesEvent<Test>> {
	let mut res = System::events()
		.into_iter()
		.rev()
		.filter_map(
			|e| if let RuntimeEvent::Bounties(inner) = e.event { Some(inner) } else { None },
		)
		.take(n)
		.collect::<Vec<_>>();
	res.reverse();
	res
}

pub fn last_event() -> BountiesEvent<Test> {
	last_events(1).into_iter().next().unwrap()
}

pub fn expect_events(e: Vec<BountiesEvent<Test>>) {
	assert_eq!(last_events(e.len()), e);
}

pub fn get_payment_id(
	parent_bounty_id: BountyIndex,
	child_bounty_id: Option<BountyIndex>,
	dest: Option<u128>,
) -> Option<u64> {
	let status =
		pallet_bounties::Pallet::<Test>::get_bounty_status(parent_bounty_id, child_bounty_id)
			.expect("should return bounty status");

	match status {
		BountyStatus::FundingAttempted {
			payment_status: PaymentState::Attempted { id }, ..
		} => Some(id),
		BountyStatus::RefundAttempted {
			payment_status: PaymentState::Attempted { id }, ..
		} => Some(id),
		BountyStatus::PayoutAttempted { curator_stash, beneficiary, .. } =>
			dest.and_then(|account| {
				if account == curator_stash.0 {
					if let PaymentState::Attempted { id } = curator_stash.1 {
						return Some(id);
					}
				} else if account == beneficiary.0 {
					if let PaymentState::Attempted { id } = beneficiary.1 {
						return Some(id);
					}
				}
				None
			}),
		_ => None,
	}
}

pub fn approve_payment(
	dest: u128,
	parent_bounty_id: BountyIndex,
	child_bounty_id: Option<BountyIndex>,
	asset_kind: u32,
	amount: u64,
) {
	assert_eq!(paid(dest, asset_kind), amount);
	let payment_id =
		get_payment_id(parent_bounty_id, child_bounty_id, Some(dest)).expect("no payment attempt");
	set_status(payment_id, PaymentStatus::Success);
	assert_ok!(Bounties::check_status(RuntimeOrigin::signed(0), parent_bounty_id, child_bounty_id));
}

pub fn reject_payment(
	dest: u128,
	parent_bounty_id: BountyIndex,
	child_bounty_id: Option<BountyIndex>,
	asset_kind: u32,
	amount: u64,
) {
	unpay(dest, asset_kind, amount);
	let payment_id =
		get_payment_id(parent_bounty_id, child_bounty_id, Some(dest)).expect("no payment attempt");
	set_status(payment_id, PaymentStatus::Failure);
	assert_ok!(Bounties::check_status(RuntimeOrigin::signed(0), parent_bounty_id, child_bounty_id));
}

#[derive(Clone)]
pub struct TestBounty {
	pub parent_bounty_id: BountyIndex,
	pub child_bounty_id: BountyIndex,
	pub asset_kind: u32,
	pub value: u64,
	pub child_value: u64,
	pub fee: u64,
	pub child_fee: u64,
	pub curator: u128,
	pub child_curator: u128,
	pub curator_deposit: u64,
	pub curator_stash: u128,
	pub child_curator_stash: u128,
	pub beneficiary: u128,
	pub child_beneficiary: u128,
}

pub fn setup_bounty() -> TestBounty {
	let asset_kind = 1;
	let value = 50;
	let child_value = 10;
	let fee = 10;
	let child_fee = 6;
	let curator = 4;
	let child_curator = 8;
	let curator_stash = 7;
	let child_curator_stash = 10;
	let beneficiary = 5;
	let child_beneficiary = 9;
	let expected_deposit = Bounties::calculate_curator_deposit(&fee, asset_kind).unwrap();
	Balances::make_free_balance_be(&curator, 100);
	Balances::make_free_balance_be(&child_curator, 100);

	TestBounty {
		parent_bounty_id: 0,
		child_bounty_id: 0,
		asset_kind,
		value,
		child_value,
		fee,
		child_fee,
		curator,
		child_curator,
		curator_deposit: expected_deposit,
		curator_stash,
		child_curator_stash,
		beneficiary,
		child_beneficiary,
	}
}

pub fn create_parent_bounty() -> TestBounty {
	let mut s = setup_bounty();

	assert_ok!(Bounties::fund_bounty(
		RuntimeOrigin::root(),
		Box::new(s.asset_kind),
		s.value,
		s.curator,
		s.fee,
		b"1234567890".to_vec()
	));
	let parent_bounty_id = pallet_bounties::BountyCount::<Test>::get() - 1;
	s.parent_bounty_id = parent_bounty_id;

	s
}

pub fn create_funded_parent_bounty() -> TestBounty {
	let s = create_parent_bounty();

	let parent_bounty_account = Bounties::bounty_account(s.parent_bounty_id, s.asset_kind.clone())
		.expect("conversion failed");
	approve_payment(parent_bounty_account, s.parent_bounty_id, None, s.asset_kind.clone(), s.value);

	s
}

pub fn create_active_parent_bounty() -> TestBounty {
	let mut s = create_funded_parent_bounty();

	assert_ok!(Bounties::accept_curator(
		RuntimeOrigin::signed(s.curator),
		s.parent_bounty_id,
		None,
		s.curator_stash
	));

	s
}

pub fn create_parent_bounty_with_unassigned_curator() -> TestBounty {
	let mut s = create_funded_parent_bounty();

	assert_ok!(Bounties::unassign_curator(
		RuntimeOrigin::signed(s.curator),
		s.parent_bounty_id,
		None,
	));

	s
}

pub fn create_awarded_parent_bounty() -> TestBounty {
	let mut s = create_active_parent_bounty();

	assert_ok!(Bounties::award_bounty(
		RuntimeOrigin::signed(s.curator),
		s.parent_bounty_id,
		None,
		s.beneficiary,
	));

	s
}

pub fn create_canceled_parent_bounty() -> TestBounty {
	let mut s = create_active_parent_bounty();

	assert_ok!(Bounties::close_bounty(RuntimeOrigin::root(), s.parent_bounty_id, None,));

	s
}

pub fn create_child_bounty_with_curator() -> TestBounty {
	let mut s = create_active_parent_bounty();

	assert_ok!(Bounties::fund_child_bounty(
		RuntimeOrigin::signed(s.curator),
		s.parent_bounty_id,
		s.child_value,
		Some(s.child_curator),
		Some(s.child_fee),
		b"1234567890".to_vec()
	));
	s.child_bounty_id =
		pallet_bounties::TotalChildBountiesPerParent::<Test>::get(s.parent_bounty_id) - 1;

	s
}

pub fn create_child_bounty_without_curator() -> TestBounty {
	let mut s = create_active_parent_bounty();

	assert_ok!(Bounties::fund_child_bounty(
		RuntimeOrigin::signed(s.curator),
		s.parent_bounty_id,
		s.child_value,
		None,
		None,
		b"1234567890".to_vec()
	));
	s.child_bounty_id =
		pallet_bounties::TotalChildBountiesPerParent::<Test>::get(s.parent_bounty_id) - 1;

	s
}
