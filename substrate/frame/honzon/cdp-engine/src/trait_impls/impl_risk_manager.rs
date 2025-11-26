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

//! Implementation of the RiskManager trait for the CDP Engine pallet.
use crate::*;
use frame_support::traits::honzon::RiskManager;
use pallet_loans::{BalanceOf, DebitUnitOf};
use sp_runtime::DispatchResult;

impl<T: Config> RiskManager<DebitUnitOf<T>, BalanceOf<T>, Rate> for Pallet<T> {
	fn debit_units_to_value(debit_units: DebitUnitOf<T>, stability_fee: Rate) -> BalanceOf<T> {
		Self::do_debit_units_to_value(debit_units, stability_fee)
	}

	fn value_to_debit_units(debit_value: BalanceOf<T>, stability_fee: Rate) -> DebitUnitOf<T> {
		Self::do_debit_value_to_units(debit_value, stability_fee)
	}

	fn check_position_valid(
		collateral_balance: BalanceOf<T>,
		debit_units: DebitUnitOf<T>,
		debit_interest_rate: Rate,
		check_required_ratio: bool,
	) -> DispatchResult {
		Self::do_check_position(
			collateral_balance,
			debit_units,
			debit_interest_rate,
			check_required_ratio,
		)
		.map_err(Into::into)
	}

	fn check_debit_cap() -> DispatchResult {
		Self::do_check_debit_cap().map_err(Into::into)
	}
}
