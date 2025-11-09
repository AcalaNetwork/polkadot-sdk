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

//! Unit tests for the loans module.

use super::*;
use frame_support::{
	assert_noop, assert_ok,
	traits::{
		fungible::{hold::Inspect as HoldInspect, Inspect},
		honzon::{DebitUnit, Rate},
	},
};
use mock::{RuntimeEvent, *};
use sp_arithmetic::traits::{One, Zero};
use sp_runtime::{ArithmeticError, DispatchError, FixedU128, TokenError};

// Helper to compute total positions
fn total_positions() -> (u128, u128) {
	use frame_support::traits::honzon::RiskManager;
	use mock::MockRiskManager;

	let mut total_debit_value = 0u128;
	let mut total_collateral = 0u128;
	Positions::<Runtime>::iter().for_each(|(_who, position)| {
		let debit_value = MockRiskManager::debit_units_to_value(
			position.debit.stability_fee,
			position.debit.units,
		);
		total_debit_value += debit_value;
		total_collateral += position.collateral;
	});
	(total_debit_value, total_collateral)
}

// Helper to get debit value from position
fn debit_value(
	position: &frame_support::traits::honzon::Position<DebitUnit<u128>, u128, FixedU128>,
) -> u128 {
	use frame_support::traits::honzon::RiskManager;
	use mock::MockRiskManager;
	MockRiskManager::debit_units_to_value(position.debit.stability_fee, position.debit.units)
}

// Helper to get debit units value (for DebitUnit comparisons)
fn debit_units_value(units: DebitUnit<u128>) -> u128 {
	use frame_support::traits::honzon::RiskManager;
	use mock::MockRiskManager;
	// For Rate::one(), the value equals units
	MockRiskManager::debit_units_to_value(Rate::one(), units)
}

#[test]
fn check_update_loan_underflow_work() {
	ExtBuilder::default().build().execute_with(|| {
		// Create a position first to avoid inc_consumers being called during underflow test
		// This prevents storage mutations from inc_consumers when testing underflow
		assert_ok!(Loans::update_loan(
			&ALICE,
			BalanceAdjustment::Increase(100),
			BalanceAdjustment::Increase(50)
		));

		// collateral underflow
		assert_noop!(
			Loans::update_loan(
				&ALICE,
				BalanceAdjustment::Decrease(200),
				BalanceAdjustment::Increase(0)
			),
			DispatchError::Arithmetic(ArithmeticError::Underflow),
		);

		// debit underflow
		assert_noop!(
			Loans::update_loan(
				&ALICE,
				BalanceAdjustment::Increase(0),
				BalanceAdjustment::Decrease(100)
			),
			DispatchError::Arithmetic(ArithmeticError::Underflow),
		);
	});
}

#[test]
fn update_loan_should_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_eq!(NativeFungible::balance(&Loans::account_id()), 0);
		assert_eq!(NativeFungible::balance(&ALICE), 10000);
		let (total_debit, total_collateral) = total_positions();
		assert_eq!(total_debit, 0);
		assert_eq!(total_collateral, 0);
		assert_eq!(debit_value(&Loans::positions(ALICE)), 0);
		assert_eq!(Loans::positions(ALICE).collateral, 0);
		assert!(!<Positions<Runtime>>::contains_key(ALICE));

		let alice_ref_count_0 = System::consumers(&ALICE);

		assert_ok!(Loans::update_loan(
			&ALICE,
			BalanceAdjustment::Increase(3000),
			BalanceAdjustment::Increase(2000)
		));

		// just update records
		let (total_debit, total_collateral) = total_positions();
		assert_eq!(total_debit, 2000);
		assert_eq!(total_collateral, 3000);
		assert_eq!(debit_value(&Loans::positions(ALICE)), 2000);
		assert_eq!(Loans::positions(ALICE).collateral, 3000);
		assert_eq!(Loans::positions(ALICE).debit.stability_fee, Rate::zero());
		assert_eq!(debit_units_value(Loans::total_debit_by_stability_fee(Rate::zero())), 2000);

		// increase ref count when open new position
		let alice_ref_count_1 = System::consumers(&ALICE);
		assert_eq!(alice_ref_count_1, alice_ref_count_0 + 1);

		// dot not manipulate balance
		assert_eq!(NativeFungible::balance(&Loans::account_id()), 0);
		assert_eq!(NativeFungible::balance(&ALICE), 10000);

		// should remove position storage if zero
		assert!(<Positions<Runtime>>::contains_key(ALICE));
		assert_ok!(Loans::update_loan(
			&ALICE,
			BalanceAdjustment::Decrease(3000),
			BalanceAdjustment::Decrease(2000)
		));
		assert_eq!(debit_value(&Loans::positions(ALICE)), 0);
		assert_eq!(Loans::positions(ALICE).collateral, 0);
		assert_eq!(debit_units_value(Loans::total_debit_by_stability_fee(Rate::one())), 0);
		assert!(!<Positions<Runtime>>::contains_key(ALICE));

		// decrease ref count after remove position
		let alice_ref_count_2 = System::consumers(&ALICE);
		assert_eq!(alice_ref_count_2, alice_ref_count_1 - 1);
	});
}

#[test]
fn transfer_loan_should_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(Loans::update_loan(
			&ALICE,
			BalanceAdjustment::Increase(1000),
			BalanceAdjustment::Increase(500)
		));
		assert_ok!(Loans::update_loan(
			&BOB,
			BalanceAdjustment::Increase(1200),
			BalanceAdjustment::Increase(600)
		));
		assert_eq!(debit_value(&Loans::positions(ALICE)), 500);
		assert_eq!(Loans::positions(ALICE).collateral, 1000);
		assert_eq!(Loans::positions(ALICE).debit.stability_fee, Rate::zero());
		assert_eq!(debit_value(&Loans::positions(BOB)), 600);
		assert_eq!(Loans::positions(BOB).collateral, 1200);
		assert_eq!(Loans::positions(BOB).debit.stability_fee, Rate::zero());

		assert_ok!(Loans::transfer_loan(&ALICE, &BOB));
		assert_eq!(debit_value(&Loans::positions(ALICE)), 0);
		assert_eq!(Loans::positions(ALICE).collateral, 0);
		assert_eq!(Loans::positions(ALICE).debit.stability_fee, Rate::zero());
		assert_eq!(debit_value(&Loans::positions(BOB)), 1100);
		assert_eq!(Loans::positions(BOB).collateral, 2200);
		assert_eq!(Loans::positions(BOB).debit.stability_fee, Rate::zero());
		assert_eq!(debit_units_value(Loans::total_debit_by_stability_fee(Rate::zero())), 1100);
		System::assert_last_event(RuntimeEvent::Loans(Event::TransferLoan {
			from: ALICE,
			to: BOB,
		}));
	});
}

#[test]
fn confiscate_collateral_and_debit_work() {
	ExtBuilder::default().build().execute_with(|| {
		let _hold_reason = RuntimeHoldReason::from(HoldReason::Collateral);
		// Setup position using update_loan
		assert_ok!(Loans::update_loan(
			&BOB,
			BalanceAdjustment::Increase(5000),
			BalanceAdjustment::Increase(1000)
		));
		assert_eq!(NativeFungible::balance(&Loans::account_id()), 0);

		// attempting to confiscate more collateral than held should fail
		assert_noop!(
			Loans::confiscate_collateral_and_debit(&BOB, 6000, 1000),
			DispatchError::Token(TokenError::Frozen)
		);
	});
}

#[test]
fn confiscate_collateral_and_debit_work_success() {
	ExtBuilder::default().build().execute_with(|| {
		let hold_reason = RuntimeHoldReason::from(HoldReason::Collateral);
		// Setup position using update_loan - note: update_loan doesn't hold collateral
		// so we need to manually hold it for this test to work
		assert_ok!(Loans::update_loan(
			&ALICE,
			BalanceAdjustment::Increase(500),
			BalanceAdjustment::Increase(200)
		));
		// Manually hold collateral since update_loan doesn't do it
		assert_ok!(NativeFungible::hold(&hold_reason, &ALICE, 500));
		assert_eq!(CDPTreasuryModule::total_collaterals(), 0);
		assert_eq!(CDPTreasuryModule::debit_pool(), 0);
		assert_eq!(debit_value(&Loans::positions(ALICE)), 200);
		assert_eq!(Loans::positions(ALICE).collateral, 500);
		assert_eq!(NativeFungible::balance_on_hold(&hold_reason, &ALICE), 500);

		assert_ok!(Loans::confiscate_collateral_and_debit(&ALICE, 300, 200));
		assert_eq!(CDPTreasuryModule::total_collaterals(), 300);
		let debit_pool = CDPTreasuryModule::debit_pool();
		assert_eq!(debit_pool, 200);
		assert_eq!(debit_value(&Loans::positions(ALICE)), 0);
		assert_eq!(Loans::positions(ALICE).collateral, 200);
		assert_eq!(NativeFungible::balance_on_hold(&hold_reason, &ALICE), 200);
		System::assert_last_event(RuntimeEvent::Loans(Event::ConfiscateCollateralAndDebit {
			owner: ALICE,
			confiscated_collateral_amount: 300,
			deduct_debit_value: 200,
		}));
	});
}
