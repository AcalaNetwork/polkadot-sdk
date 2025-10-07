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

//! Unit tests for the honzon module.

#![cfg(test)]

use crate::mock::{Honzon, ExtBuilder, RuntimeOrigin, ALICE};
use frame_support::{assert_noop, assert_ok};
use pallet_traits::Rate;
use sp_runtime::DispatchError;

fn new_test_ext() -> sp_io::TestExternalities {
    ExtBuilder::default().build()
}

#[test]
fn set_stability_fee_should_work_for_admin() {
    new_test_ext().execute_with(|| {
        let new_fee = Rate::from_rational(1, 100); // 1%
        assert_ok!(Honzon::set_stability_fee(
            RuntimeOrigin::root(),
            ALICE,
            new_fee
        ));
    });
}

#[test]
fn set_stability_fee_should_fail_for_non_admin() {
    new_test_ext().execute_with(|| {
        let new_fee = Rate::from_rational(1, 100); // 1%
        assert_noop!(
            Honzon::set_stability_fee(RuntimeOrigin::signed(ALICE), ALICE, new_fee),
            DispatchError::BadOrigin
        );
    });
}
