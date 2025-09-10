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

// TODO: this is not a pallet, but a traits library, need to rename it or break it

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::upper_case_acronyms)]
#![allow(clippy::from_over_into)]
#![allow(clippy::type_complexity)]

use codec::{Decode, Encode, MaxEncodedLen};
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use scale_info::TypeInfo;
use sp_runtime::{traits::CheckedDiv, DispatchError, DispatchResult, FixedU128, RuntimeDebug};
use sp_std::prelude::*;

pub mod auction;
pub mod bounded;
pub mod dex;
pub mod honzon;
pub mod data_provider;

pub use crate::auction::*;
pub use crate::bounded::*;
pub use crate::dex::*;
pub use crate::honzon::*;
pub use crate::data_provider::*;

pub type Price = FixedU128;
pub type ExchangeRate = FixedU128;
pub type Ratio = FixedU128;
pub type Rate = FixedU128;

/// Indicate if should change a value
#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum Change<Value> {
	/// No change.
	NoChange,
	/// Changed to new value.
	NewValue(Value),
}

#[derive(Encode, Decode, RuntimeDebug, Eq, PartialEq, Ord, PartialOrd, Clone, Copy)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct TimestampedValue<Value: Ord + PartialOrd, Moment> {
	pub value: Value,
	pub timestamp: Moment,
}


/// Combine data provided by operators
pub trait CombineData<Key, TimestampedValue> {
	/// Combine data provided by operators
	fn combine_data(
		key: &Key,
		values: Vec<TimestampedValue>,
		prev_value: Option<TimestampedValue>,
	) -> Option<TimestampedValue>;
}

/// New data handler
#[impl_trait_for_tuples::impl_for_tuples(30)]
pub trait OnNewData<AccountId, Key, Value> {
	/// New data is available
	fn on_new_data(who: &AccountId, key: &Key, value: &Value);
}


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
