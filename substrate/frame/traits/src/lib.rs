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

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::upper_case_acronyms)]
#![allow(clippy::from_over_into)]
#![allow(clippy::type_complexity)]

use sp_runtime::{traits::CheckedDiv, DispatchError, DispatchResult, FixedU128};
use sp_std::prelude::*;

pub mod bounded;
pub mod dex;
pub mod honzon;

pub use crate::bounded::*;
pub use crate::dex::*;
pub use crate::honzon::*;

pub type Price = FixedU128;
pub type ExchangeRate = FixedU128;
pub type Ratio = FixedU128;
pub type Rate = FixedU128;

pub trait PriceProvider<CurrencyId> {
	fn get_price(currency_id: CurrencyId) -> Option<Price>;
	fn get_relative_price(base: CurrencyId, quote: CurrencyId) -> Option<Price> {
		if let (Some(base_price), Some(quote_price)) =
			(Self::get_price(base), Self::get_price(quote))
		{
			base_price.checked_div(&quote_price)
		} else {
			None
		}
	}
}

pub trait DEXPriceProvider<CurrencyId> {
	fn get_relative_price(base: CurrencyId, quote: CurrencyId) -> Option<ExchangeRate>;
}

pub trait LockablePrice<CurrencyId> {
	fn lock_price(currency_id: CurrencyId) -> DispatchResult;
	fn unlock_price(currency_id: CurrencyId) -> DispatchResult;
}

pub trait ExchangeRateProvider {
	fn get_exchange_rate() -> ExchangeRate;
}
pub trait LiquidateCollateral<AccountId, CurrencyId, Balance> {
	fn liquidate(
		who: &AccountId,
		currency_id: CurrencyId,
		amount: Balance,
		target_stable_amount: Balance,
	) -> DispatchResult;
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl<AccountId, CurrencyId: Clone, Balance: Clone>
	LiquidateCollateral<AccountId, CurrencyId, Balance> for Tuple
{
	fn liquidate(
		who: &AccountId,
		currency_id: CurrencyId,
		amount: Balance,
		target_stable_amount: Balance,
	) -> DispatchResult {
		let mut last_error = None;
		for_tuples!( #(
			match Tuple::liquidate(who, currency_id.clone(), amount.clone(), target_stable_amount.clone()) {
				Ok(_) => return Ok(()),
				Err(e) => { last_error = Some(e) }
			}
		)* );
		let last_error = last_error.unwrap_or(DispatchError::Other("No liquidation impl."));
		Err(last_error)
	}
}
