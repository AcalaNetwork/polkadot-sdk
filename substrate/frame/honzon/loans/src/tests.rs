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

//! Unit tests for the loans module.

#![cfg(test)]

use super::*;
use frame_support::traits::fungible::hold::Inspect as HoldInspect;
use frame_support::traits::fungible::Inspect;
use frame_support::{assert_noop, assert_ok};
use mock::{RuntimeEvent, *};
use pallet_traits::{Rate, Ratio};
use sp_arithmetic::traits::{One, Zero};
use sp_runtime::{ArithmeticError, DispatchError, TokenError};

#[test]
fn debits_key() {
	ExtBuilder::default().build().execute_with(|| {
		let hold_reason = RuntimeHoldReason::from(HoldReason::Collateral);
		assert_eq!(Collateral::balance(&ALICE), 10000);
		assert_eq!(Collateral::balance(&Loans::account_id()), 0);
		assert_eq!(Collateral::balance_on_hold(&hold_reason, &ALICE), 0);
		assert_eq!(Loans::positions(&ALICE).debit, 0);
		assert_ok!(Loans::adjust_position(&ALICE, 200, 100, Some(Rate::one())));
		assert_eq!(Loans::positions(&ALICE).debit, 100);
		assert_eq!(Collateral::balance(&ALICE), 9800);
		assert_eq!(Collateral::balance(&Loans::account_id()), 0);
		assert_eq!(Collateral::balance_on_hold(&hold_reason, &ALICE), 200);
		assert_ok!(Loans::adjust_position(&ALICE, -100, -50, None));
		assert_eq!(Loans::positions(&ALICE).debit, 50);
		assert_eq!(Collateral::balance_on_hold(&hold_reason, &ALICE), 100);
	});
}

#[test]
fn check_update_loan_underflow_work() {
	ExtBuilder::default().build().execute_with(|| {
		// collateral underflow
		assert_noop!(
			Loans::update_loan(&ALICE, -100, 0, None),
			DispatchError::Arithmetic(ArithmeticError::Underflow),
		);

		// debit underflow
		assert_noop!(
			Loans::update_loan(&ALICE, 0, -100, None),
			DispatchError::Arithmetic(ArithmeticError::Underflow),
		);
	});
}

#[test]
fn adjust_position_should_work() {
	ExtBuilder::default().build().execute_with(|| {
		let hold_reason = RuntimeHoldReason::from(HoldReason::Collateral);
		assert_eq!(Collateral::balance(&ALICE), 10000);
		assert_eq!(Collateral::balance_on_hold(&hold_reason, &ALICE), 0);

		// balance too low
		assert_noop!(
			Loans::adjust_position(&ALICE, 20000, 0, None),
			DispatchError::Token(TokenError::FundsUnavailable)
		);

		// mock can't pass required ratio check
		assert_noop!(
			Loans::adjust_position(&ALICE, 500, 300, Some(Rate::one())),
			sp_runtime::DispatchError::Other("mock below required collateral ratio error")
		);

		// mock exceed debit value cap
		assert_noop!(
			Loans::adjust_position(&ALICE, 2000, 1100, Some(Rate::one())),
			sp_runtime::DispatchError::Other("mock exceed debit value cap error")
		);

		assert_eq!(Collateral::balance(&ALICE), 10000);
		assert_eq!(Collateral::balance(&Loans::account_id()), 0);
		assert_eq!(Collateral::balance_on_hold(&hold_reason, &ALICE), 0);
		assert_eq!(Loans::total_positions().debit, 0);
		assert_eq!(Loans::total_positions().collateral, 0);
		assert_eq!(Loans::positions(&ALICE).debit, 0);
		assert_eq!(Loans::positions(&ALICE).collateral, 0);

		// success
		assert_ok!(Loans::adjust_position(&ALICE, 500, 200, Some(Rate::one())));
		assert_eq!(Collateral::balance(&ALICE), 9500);
		assert_eq!(Collateral::balance(&Loans::account_id()), 0);
		assert_eq!(Collateral::balance_on_hold(&hold_reason, &ALICE), 500);
		assert_eq!(Loans::total_positions().debit, 200);
		assert_eq!(Loans::total_positions().collateral, 500);
		assert_eq!(Loans::positions(&ALICE).debit, 200);
		assert_eq!(Loans::positions(&ALICE).collateral, 500);
		assert_eq!(Loans::positions(&ALICE).stability_fee, Rate::one());
		System::assert_has_event(RuntimeEvent::Loans(Event::PositionUpdated {
			owner: ALICE,
			collateral_adjustment: 500,
			debit_adjustment: 200,
		}));

		// collateral_adjustment is negatives
		assert_ok!(Loans::adjust_position(&ALICE, -500, -200, None));
		assert_eq!(Collateral::balance(&Loans::account_id()), 0);
		assert_eq!(Collateral::balance_on_hold(&hold_reason, &ALICE), 0);
		assert_eq!(Collateral::balance(&ALICE), 10000);
	});
}

#[test]
fn update_loan_should_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_eq!(Collateral::balance(&Loans::account_id()), 0);
		assert_eq!(Collateral::balance(&ALICE), 10000);
		assert_eq!(Loans::total_positions().debit, 0);
		assert_eq!(Loans::total_positions().collateral, 0);
		assert_eq!(Loans::positions(&ALICE).debit, 0);
		assert_eq!(Loans::positions(&ALICE).collateral, 0);
		assert!(!<Positions<Runtime>>::contains_key(&ALICE));

		let alice_ref_count_0 = System::consumers(&ALICE);

		assert_ok!(Loans::update_loan(&ALICE, 3000, 2000, Some(Rate::one())));

		// just update records
		assert_eq!(Loans::total_positions().debit, 2000);
		assert_eq!(Loans::total_positions().collateral, 3000);
		assert_eq!(Loans::positions(&ALICE).debit, 2000);
		assert_eq!(Loans::positions(&ALICE).collateral, 3000);
		assert_eq!(Loans::positions(&ALICE).stability_fee, Rate::one());
		assert_eq!(Loans::total_debit_by_stability_fee(Rate::one()), 2000);

		// increase ref count when open new position
		let alice_ref_count_1 = System::consumers(&ALICE);
		assert_eq!(alice_ref_count_1, alice_ref_count_0 + 1);

		// dot not manipulate balance
		assert_eq!(Collateral::balance(&Loans::account_id()), 0);
		assert_eq!(Collateral::balance(&ALICE), 10000);

		// should remove position storage if zero
		assert!(<Positions<Runtime>>::contains_key(&ALICE));
		assert_ok!(Loans::update_loan(&ALICE, -3000, -2000, None));
		assert_eq!(Loans::positions(&ALICE).debit, 0);
		assert_eq!(Loans::positions(&ALICE).collateral, 0);
		assert_eq!(Loans::total_debit_by_stability_fee(Rate::one()), 0);
		assert!(!<Positions<Runtime>>::contains_key(&ALICE));

		// decrease ref count after remove position
		let alice_ref_count_2 = System::consumers(&ALICE);
		assert_eq!(alice_ref_count_2, alice_ref_count_1 - 1);
	});
}

#[test]
fn transfer_loan_should_work() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(Loans::update_loan(&ALICE, 1000, 500, Some(Rate::one())));
		assert_ok!(Loans::update_loan(&BOB, 1200, 600, Some(Rate::one())));
		assert_eq!(Loans::positions(&ALICE).debit, 500);
		assert_eq!(Loans::positions(&ALICE).collateral, 1000);
		assert_eq!(Loans::positions(&ALICE).stability_fee, Rate::one());
		assert_eq!(Loans::positions(&BOB).debit, 600);
		assert_eq!(Loans::positions(&BOB).collateral, 1200);
		assert_eq!(Loans::positions(&BOB).stability_fee, Rate::one());

		assert_ok!(Loans::transfer_loan(&ALICE, &BOB));
		assert_eq!(Loans::positions(&ALICE).debit, 0);
		assert_eq!(Loans::positions(&ALICE).collateral, 0);
		assert_eq!(Loans::positions(&ALICE).stability_fee, Ratio::zero());
		assert_eq!(Loans::positions(&BOB).debit, 1100);
		assert_eq!(Loans::positions(&BOB).collateral, 2200);
		assert_eq!(Loans::positions(&BOB).stability_fee, Rate::one());
		assert_eq!(Loans::total_debit_by_stability_fee(Rate::one()), 1100);
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
		assert_ok!(Loans::adjust_position(&BOB, 5000, 1000, Some(Rate::one())));
		assert_eq!(Collateral::balance(&Loans::account_id()), 0);

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
		assert_ok!(Loans::adjust_position(&ALICE, 500, 200, Some(Rate::one())));
		assert_eq!(CDPTreasuryModule::total_collaterals(), 0);
		assert_eq!(CDPTreasuryModule::debit_pool(), 0);
		assert_eq!(Loans::positions(&ALICE).debit, 200);
		assert_eq!(Loans::positions(&ALICE).collateral, 500);
		assert_eq!(Collateral::balance_on_hold(&hold_reason, &ALICE), 500);

		assert_ok!(Loans::confiscate_collateral_and_debit(&ALICE, 300, 200));
		assert_eq!(CDPTreasuryModule::total_collaterals(), 300);
		let debit_pool = CDPTreasuryModule::debit_pool();
		assert_eq!(debit_pool, 100);
		assert_eq!(Loans::positions(&ALICE).debit, 0);
		assert_eq!(Loans::positions(&ALICE).collateral, 200);
		assert_eq!(Collateral::balance_on_hold(&hold_reason, &ALICE), 200);
		System::assert_last_event(RuntimeEvent::Loans(Event::ConfiscateCollateralAndDebit {
			owner: ALICE,
			confiscated_collateral_amount: 300,
			deduct_debit_amount: 200,
		}));
	});
}
