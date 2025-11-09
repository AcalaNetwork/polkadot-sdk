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
use pallet_loans::BalanceOf;
use sp_runtime::DispatchResult;

impl<T: Config> RiskManager<AccountIdOf<T>, CurrencyIdOf<T>, Rate, BalanceOf<T>, BalanceOf<T>>
	for Pallet<T>
{
	fn get_debit_value(
		_currency_id: CurrencyIdOf<T>,
		interest_rate_per_sec: Rate,
		debit_balance: BalanceOf<T>,
	) -> BalanceOf<T> {
		Pallet::<T>::get_debit_value(interest_rate_per_sec, debit_balance)
	}

	fn check_position_valid(
		_currency_id: CurrencyIdOf<T>,
		collateral_balance: BalanceOf<T>,
		debit_balance: BalanceOf<T>,
		stability_fee: Rate,
		check_required_ratio: bool,
	) -> DispatchResult {
		Self::do_check_position(
			collateral_balance,
			debit_balance,
			stability_fee,
			check_required_ratio,
		)
		.map_err(Into::into)
	}

	fn check_debit_cap(_currency_id: CurrencyIdOf<T>) -> DispatchResult {
		Self::do_check_debit_cap().map_err(Into::into)
	}
}
