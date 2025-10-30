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

//! Types for the Honzon protocol.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_runtime::{traits::CheckedDiv, DispatchError, DispatchResult, FixedU128, RuntimeDebug};
use sp_std::prelude::*;

pub trait GetByKey<Key, Value> {
	fn get(key: &Key) -> Value;
}

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

/// A value with an associated timestamp.
#[derive(Encode, Decode, RuntimeDebug, Eq, PartialEq, Ord, PartialOrd, Clone, Copy)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct TimestampedValue<Value: Ord + PartialOrd, Moment> {
	/// The value.
	pub value: Value,
	/// The timestamp.
	pub timestamp: Moment,
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

/// A limit on the amount of assets to swap.
#[derive(Encode, Decode, Eq, PartialEq, Copy, Clone, RuntimeDebug, MaxEncodedLen, TypeInfo)]
pub enum SwapLimit<Balance> {
	/// Swap with exact supply amount, and minimum target amount.
	ExactSupply(Balance, Balance),
	/// Swap with exact target amount, and maximum supply amount.
	ExactTarget(Balance, Balance),
}

/// A collateralized debt position.
#[derive(
	Encode, Decode, Eq, PartialEq, Copy, Clone, RuntimeDebug, Default, MaxEncodedLen, TypeInfo,
)]
pub struct Position<Balance> {
	/// The amount of collateral.
	pub collateral: Balance,
	/// The amount of debit.
	pub debit: Balance,
	/// The stability fee used for this position.
	pub stability_fee: Ratio,
}

/// The current status of a connection.
#[derive(Clone, Encode, Decode, MaxEncodedLen, TypeInfo, RuntimeDebug, PartialEq)]
pub enum ConnectionStatus<Balance, BlockNumber> {
	/// The connection is active and collateral can be withdrawn.
	Active,
	/// A withdrawal has been initiated and the pallet is waiting for the destination to complete
	/// it.
	WithdrawalInProgress {
		/// The amount of stables being withdrawn.
		amount: Balance,
		/// The block number when the withdrawal was initiated.
		initiated_at: BlockNumber,
	},
}

/// Represents an integrated position across a vault and a liquidity destination.
#[derive(Clone, Encode, Decode, MaxEncodedLen, TypeInfo, RuntimeDebug, PartialEq)]
pub struct Connection<AccountId, DestinationId, Balance, BlockNumber> {
	/// The owner of the connection.
	pub owner: AccountId,
	/// The destination where liquidity is deposited.
	pub destination_id: DestinationId,
	/// The current status of the connection.
	pub status: ConnectionStatus<Balance, BlockNumber>,
}
