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

//! Unit tests for the cdp treasury module.

#![cfg(test)]

use super::{module::Pallet, *};
use frame_support::{assert_noop, assert_ok};
use mock::*;
use sp_runtime::{traits::BadOrigin, ArithmeticError};

#[test]
fn surplus_pool_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_eq!(Pallet::<Test>::surplus_pool(), 0);
		assert_ok!(Assets::transfer(
			RuntimeOrigin::signed(ALICE),
			STABLE,
			Pallet::<Test>::account_id(),
			500
		));
		assert_eq!(Pallet::<Test>::surplus_pool(), 500);
	});
}

#[test]
fn total_collaterals_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_eq!(Pallet::<Test>::total_collaterals(), 0);
		assert_ok!(Assets::transfer(
			RuntimeOrigin::signed(ALICE),
			NATIVE,
			Pallet::<Test>::account_id(),
			10
		));
		assert_eq!(Pallet::<Test>::total_collaterals(), 10);
	});
}

#[test]
fn on_system_debit_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_eq!(Pallet::<Test>::debit_pool(), 0);
		assert_ok!(Pallet::<Test>::on_system_debit(1000));
		assert_eq!(Pallet::<Test>::debit_pool(), 1000);
		assert_noop!(
			Pallet::<Test>::on_system_debit(Balance::max_value()),
			ArithmeticError::Overflow,
		);
	});
}

#[test]
fn on_system_surplus_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_eq!(Assets::balance(STABLE, &Pallet::<Test>::account_id()), 0);
		assert_eq!(Pallet::<Test>::surplus_pool(), 0);
		assert_ok!(Pallet::<Test>::on_system_surplus(1000));
		assert_eq!(Assets::balance(STABLE, &Pallet::<Test>::account_id()), 1000);
		assert_eq!(Pallet::<Test>::surplus_pool(), 1000);
	});
}

#[test]
fn offset_surplus_and_debit_on_finalize_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_eq!(Assets::balance(STABLE, &Pallet::<Test>::account_id()), 0);
		assert_eq!(Pallet::<Test>::surplus_pool(), 0);
		assert_eq!(Pallet::<Test>::debit_pool(), 0);
		assert_ok!(Pallet::<Test>::on_system_surplus(1000));
		assert_eq!(Assets::balance(STABLE, &Pallet::<Test>::account_id()), 1000);
		assert_eq!(Pallet::<Test>::surplus_pool(), 1000);
		Pallet::<Test>::on_finalize(1);
		assert_eq!(Assets::balance(STABLE, &Pallet::<Test>::account_id()), 1000);
		assert_eq!(Pallet::<Test>::surplus_pool(), 1000);
		assert_eq!(Pallet::<Test>::debit_pool(), 0);
		assert_ok!(Pallet::<Test>::on_system_debit(300));
		assert_eq!(Pallet::<Test>::debit_pool(), 300);
		Pallet::<Test>::on_finalize(2);
		assert_eq!(Assets::balance(STABLE, &Pallet::<Test>::account_id()), 700);
		assert_eq!(Pallet::<Test>::surplus_pool(), 700);
		assert_eq!(Pallet::<Test>::debit_pool(), 0);
		assert_ok!(Pallet::<Test>::on_system_debit(800));
		assert_eq!(Pallet::<Test>::debit_pool(), 800);
		Pallet::<Test>::on_finalize(3);
		assert_eq!(Assets::balance(STABLE, &Pallet::<Test>::account_id()), 0);
		assert_eq!(Pallet::<Test>::surplus_pool(), 0);
		assert_eq!(Pallet::<Test>::debit_pool(), 100);
	});
}

#[test]
fn issue_debit_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_eq!(Assets::balance(STABLE, &ALICE), 100000);
		assert_eq!(Pallet::<Test>::debit_pool(), 0);

		assert_ok!(Pallet::<Test>::issue_debit(&ALICE, 1000, true));
		assert_eq!(Assets::balance(STABLE, &ALICE), 101000);
		assert_eq!(Pallet::<Test>::debit_pool(), 0);

		assert_ok!(Pallet::<Test>::issue_debit(&ALICE, 1000, false));
		assert_eq!(Assets::balance(STABLE, &ALICE), 102000);
		assert_eq!(Pallet::<Test>::debit_pool(), 1000);
	});
}

#[test]
fn burn_debit_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_eq!(Assets::balance(STABLE, &ALICE), 100000);
		assert_eq!(Pallet::<Test>::debit_pool(), 0);
		assert_ok!(Pallet::<Test>::burn_debit(&ALICE, 300));
		assert_eq!(Assets::balance(STABLE, &ALICE), 99700);
		assert_eq!(Pallet::<Test>::debit_pool(), 0);
	});
}

#[test]
fn deposit_surplus_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_eq!(Assets::balance(STABLE, &ALICE), 100000);
		assert_eq!(Assets::balance(STABLE, &Pallet::<Test>::account_id()), 0);
		assert_eq!(Pallet::<Test>::surplus_pool(), 0);
		assert_ok!(Pallet::<Test>::deposit_surplus(&ALICE, 300));
		assert_eq!(Assets::balance(STABLE, &ALICE), 99700);
		assert_eq!(Assets::balance(STABLE, &Pallet::<Test>::account_id()), 300);
		assert_eq!(Pallet::<Test>::surplus_pool(), 300);
	});
}

#[test]
fn withdraw_surplus_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(Pallet::<Test>::deposit_surplus(&ALICE, 300));
		assert_eq!(Assets::balance(STABLE, &ALICE), 99700);
		assert_eq!(Assets::balance(STABLE, &Pallet::<Test>::account_id()), 300);
		assert_eq!(Pallet::<Test>::surplus_pool(), 300);

		assert_ok!(Pallet::<Test>::withdraw_surplus(&ALICE, 200));
		assert_eq!(Assets::balance(STABLE, &ALICE), 99900);
		assert_eq!(Assets::balance(STABLE, &Pallet::<Test>::account_id()), 100);
		assert_eq!(Pallet::<Test>::surplus_pool(), 100);
	});
}

#[test]
fn deposit_collateral_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_eq!(Pallet::<Test>::total_collaterals(), 0);
		assert_eq!(Assets::balance(NATIVE, &Pallet::<Test>::account_id()), 0);
		assert_eq!(Assets::balance(NATIVE, &ALICE), 100000);
		assert!(!Pallet::<Test>::deposit_collateral(&ALICE, 100001).is_ok());
		assert_ok!(Pallet::<Test>::deposit_collateral(&ALICE, 500));
		assert_eq!(Pallet::<Test>::total_collaterals(), 500);
		assert_eq!(Assets::balance(NATIVE, &Pallet::<Test>::account_id()), 500);
		assert_eq!(Assets::balance(NATIVE, &ALICE), 99500);
	});
}

#[test]
fn withdraw_collateral_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(Pallet::<Test>::deposit_collateral(&ALICE, 500));
		assert_eq!(Pallet::<Test>::total_collaterals(), 500);
		assert_eq!(Assets::balance(NATIVE, &Pallet::<Test>::account_id()), 500);
		assert_eq!(Assets::balance(NATIVE, &BOB), 100000);
		assert!(!Pallet::<Test>::withdraw_collateral(&BOB, 501).is_ok());
		assert_ok!(Pallet::<Test>::withdraw_collateral(&BOB, 400));
		assert_eq!(Pallet::<Test>::total_collaterals(), 100);
		assert_eq!(Assets::balance(NATIVE, &Pallet::<Test>::account_id()), 100);
		assert_eq!(Assets::balance(NATIVE, &BOB), 100400);
	});
}

#[test]
fn get_total_collaterals_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(Pallet::<Test>::deposit_collateral(&ALICE, 500));
		assert_eq!(Pallet::<Test>::get_total_collaterals(), 500);
	});
}

#[test]
fn get_debit_proportion_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_eq!(
			Pallet::<Test>::get_debit_proportion(100),
			Ratio::saturating_from_rational(100, Assets::total_issuance(STABLE))
		);
	});
}

#[test]
fn create_collateral_auctions_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(Assets::transfer(
			RuntimeOrigin::signed(ALICE),
			NATIVE,
			Pallet::<Test>::account_id(),
			10000
		));
		assert_eq!(Pallet::<Test>::expected_collateral_auction_size(), 0);
		assert_noop!(
			Pallet::<Test>::create_collateral_auctions(10001, 1000, ALICE, true),
			Error::<Test>::CollateralNotEnough,
		);

		// without collateral auction maximum size
		assert_ok!(Pallet::<Test>::create_collateral_auctions(1000, 1000, ALICE, true));
		assert_eq!(total_collateral_auction(), 1);
		assert_eq!(total_collateral_in_auction(), 1000);

		// set collateral auction maximum size
		assert_ok!(Pallet::<Test>::set_expected_collateral_auction_size(
			RuntimeOrigin::signed(1),
			300
		));

		// amount < collateral auction maximum size
		// auction + 1
		assert_ok!(Pallet::<Test>::create_collateral_auctions(200, 1000, ALICE, true));
		assert_eq!(total_collateral_auction(), 2);
		assert_eq!(total_collateral_in_auction(), 1200);

		// not exceed lots count cap
		// auction + 4
		assert_ok!(Pallet::<Test>::create_collateral_auctions(1000, 1000, ALICE, true));
		assert_eq!(total_collateral_auction(), 6);
		assert_eq!(total_collateral_in_auction(), 2200);

		// exceed lots count cap
		// auction + 5
		assert_ok!(Pallet::<Test>::create_collateral_auctions(2000, 1000, ALICE, true));
		assert_eq!(total_collateral_auction(), 11);
		assert_eq!(total_collateral_in_auction(), 4200);
	});
}

#[test]
fn set_expected_collateral_auction_size_work() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		assert_eq!(Pallet::<Test>::expected_collateral_auction_size(), 0);
		assert_noop!(
			Pallet::<Test>::set_expected_collateral_auction_size(RuntimeOrigin::signed(5), 200),
			BadOrigin
		);
		assert_ok!(Pallet::<Test>::set_expected_collateral_auction_size(
			RuntimeOrigin::signed(1),
			200
		));
		System::assert_last_event(RuntimeEvent::CDPTreasuryModule(
			crate::Event::ExpectedCollateralAuctionSizeUpdated { new_size: 200 },
		));
	});
}

#[test]
fn extract_surplus_to_treasury_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(Pallet::<Test>::on_system_surplus(1000));
		assert_eq!(Pallet::<Test>::surplus_pool(), 1000);
		assert_eq!(Assets::balance(STABLE, &Pallet::<Test>::account_id()), 1000);
		assert_eq!(Assets::balance(STABLE, &TreasuryAccount::get()), 0);

		assert_noop!(
			Pallet::<Test>::extract_surplus_to_treasury(RuntimeOrigin::signed(5), 200),
			BadOrigin
		);
		assert_ok!(Pallet::<Test>::extract_surplus_to_treasury(RuntimeOrigin::signed(1), 200));
		assert_eq!(Pallet::<Test>::surplus_pool(), 800);
		assert_eq!(Assets::balance(STABLE, &Pallet::<Test>::account_id()), 800);
		assert_eq!(Assets::balance(STABLE, &TreasuryAccount::get()), 200);
	});
}

#[test]
fn auction_collateral_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(Assets::transfer(
			RuntimeOrigin::signed(ALICE),
			NATIVE,
			Pallet::<Test>::account_id(),
			10000
		));
		assert_eq!(Pallet::<Test>::expected_collateral_auction_size(), 0);
		assert_eq!(Pallet::<Test>::total_collaterals(), 10000);
		assert_eq!(Pallet::<Test>::total_collaterals_not_in_auction(), 10000);
		assert_noop!(
			Pallet::<Test>::auction_collateral(RuntimeOrigin::signed(5), 10000, 1000, false),
			BadOrigin,
		);
		assert_noop!(
			Pallet::<Test>::auction_collateral(RuntimeOrigin::signed(1), 10001, 1000, false),
			Error::<Test>::CollateralNotEnough,
		);

		assert_ok!(Pallet::<Test>::auction_collateral(RuntimeOrigin::signed(1), 1000, 1000, false));
		assert_eq!(total_collateral_auction(), 1);
		assert_eq!(total_collateral_in_auction(), 1000);

		assert_eq!(Pallet::<Test>::total_collaterals(), 10000);
		assert_eq!(Pallet::<Test>::total_collaterals_not_in_auction(), 9000);
		assert_noop!(
			Pallet::<Test>::auction_collateral(RuntimeOrigin::signed(1), 9001, 1000, false),
			Error::<Test>::CollateralNotEnough,
		);
	});
}

#[test]
fn set_debit_offset_buffer_work() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		assert_eq!(Pallet::<Test>::debit_offset_buffer(), 0);
		assert_noop!(
			Pallet::<Test>::set_debit_offset_buffer(RuntimeOrigin::signed(5), 200),
			BadOrigin
		);
		assert_ok!(Pallet::<Test>::set_debit_offset_buffer(RuntimeOrigin::signed(1), 200));
		System::assert_last_event(RuntimeEvent::CDPTreasuryModule(
			crate::Event::DebitOffsetBufferUpdated { amount: 200 },
		));
	});
}

#[test]
fn offset_surplus_and_debit_limited_by_debit_offset_buffer() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(Pallet::<Test>::on_system_surplus(1000));
		assert_ok!(Pallet::<Test>::on_system_debit(2000));
		assert_eq!(Pallet::<Test>::surplus_pool(), 1000);
		assert_eq!(Pallet::<Test>::debit_pool(), 2000);
		assert_eq!(Pallet::<Test>::debit_offset_buffer(), 0);

		// offset all debit pool when surplus is enough
		Pallet::<Test>::offset_surplus_and_debit();
		assert_eq!(Pallet::<Test>::surplus_pool(), 0);
		assert_eq!(Pallet::<Test>::debit_pool(), 1000);
		assert_eq!(Pallet::<Test>::debit_offset_buffer(), 0);

		assert_ok!(Pallet::<Test>::set_debit_offset_buffer(RuntimeOrigin::signed(1), 100));
		assert_eq!(Pallet::<Test>::debit_offset_buffer(), 100);
		assert_ok!(Pallet::<Test>::on_system_surplus(2000));
		assert_eq!(Pallet::<Test>::surplus_pool(), 2000);

		// keep the buffer for debit pool when surplus is enough
		Pallet::<Test>::offset_surplus_and_debit();
		assert_eq!(Pallet::<Test>::surplus_pool(), 1100);
		assert_eq!(Pallet::<Test>::debit_pool(), 100);
		assert_eq!(Pallet::<Test>::debit_offset_buffer(), 100);

		assert_ok!(Pallet::<Test>::set_debit_offset_buffer(RuntimeOrigin::signed(1), 200));
		assert_eq!(Pallet::<Test>::debit_offset_buffer(), 200);
		assert_ok!(Pallet::<Test>::on_system_debit(1400));
		assert_eq!(Pallet::<Test>::debit_pool(), 1500);

		Pallet::<Test>::offset_surplus_and_debit();
		assert_eq!(Pallet::<Test>::surplus_pool(), 0);
		assert_eq!(Pallet::<Test>::debit_pool(), 400);
		assert_eq!(Pallet::<Test>::debit_offset_buffer(), 200);
	});
}
