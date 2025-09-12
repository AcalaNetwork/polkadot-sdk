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

#![doc = "This crate provides a collection of shared traits and types for the Honzon protocol and its related pallets."]
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

/// The price of a currency, represented as a `FixedU128`.
pub type Price = FixedU128;
/// The exchange rate between two currencies, represented as a `FixedU128`.
pub type ExchangeRate = FixedU128;
/// A ratio, represented as a `FixedU128`.
pub type Ratio = FixedU128;
/// A rate, represented as a `FixedU128`.
pub type Rate = FixedU128;

/// A generic handler for implementing the Chain of Responsibility pattern.
pub trait Handler<T> {
	/// Handles a given value.
	fn handle(t: &T) -> DispatchResult;
}

#[impl_trait_for_tuples::impl_for_tuples(30)]
impl<T> Handler<T> for Tuple {
	fn handle(t: &T) -> DispatchResult {
		for_tuples!( #( Tuple::handle(t); )* );
		Ok(())
	}
}


/// Represents a potential change to a value.
#[derive(Encode, Decode, Clone, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum Change<Value> {
	/// No change is required.
	NoChange,
	/// The value should be changed to the new value.
	NewValue(Value),
}

/// A value with an associated timestamp.
#[derive(Encode, Decode, RuntimeDebug, Eq, PartialEq, Ord, PartialOrd, Clone, Copy)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct TimestampedValue<Value: Ord + PartialOrd, Moment> {
	/// The value.
	pub value: Value,
	/// The timestamp.
	pub timestamp: Moment,
}


/// Used to combine data from multiple providers.
pub trait CombineData<Key, TimestampedValue> {
	/// Combine data provided by operators
	fn combine_data(
		key: &Key,
		values: Vec<TimestampedValue>,
		prev_value: Option<TimestampedValue>,
	) -> Option<TimestampedValue>;
}

/// A handler for new data events.
#[impl_trait_for_tuples::impl_for_tuples(30)]
pub trait OnNewData<AccountId, Key, Value> {
	/// New data is available
	fn on_new_data(who: &AccountId, key: &Key, value: &Value);
}


/// A trait for providing the price of a currency.
pub trait PriceProvider<CurrencyId> {
	/// Returns the price of a currency.
	fn get_price(currency_id: CurrencyId) -> Option<Price>;
	/// Returns the relative price of two currencies.
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

/// Provides a relative price from a DEX.
pub trait DEXPriceProvider<CurrencyId> {
	/// Get the relative price of two currencies from a DEX.
	fn get_relative_price(base: CurrencyId, quote: CurrencyId) -> Option<ExchangeRate>;
}

/// Used to lock and unlock prices.
pub trait LockablePrice<CurrencyId> {
	/// Lock the price of a currency.
	fn lock_price(currency_id: CurrencyId) -> DispatchResult;
	/// Unlock the price of a currency.
	fn unlock_price(currency_id: CurrencyId) -> DispatchResult;
}

/// Provides a generic exchange rate.
pub trait ExchangeRateProvider {
	/// Get the exchange rate.
	fn get_exchange_rate() -> ExchangeRate;
}

/// A trait for liquidating collateral.
pub trait LiquidateCollateral<AccountId, CurrencyId, Balance> {
	/// Liquidates a specified amount of collateral.
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
