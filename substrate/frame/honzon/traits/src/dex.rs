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

//! This module provides traits for interacting with a Decentralized Exchange (DEX).

use codec::{Decode, Encode};
use frame_support::{ensure, traits::Get};
use scale_info::TypeInfo;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use sp_core::H160;
use sp_runtime::{DispatchError, DispatchResult, RuntimeDebug};
use sp_std::{cmp::PartialEq, prelude::*, result::Result};

/// Specifies the limit for a swap operation.
#[derive(RuntimeDebug, Encode, Decode, Clone, Copy, PartialEq, Eq, TypeInfo)]
pub enum SwapLimit<Balance> {
	/// Swaps an exact amount of the supply currency for a minimum amount of the target currency.
	/// The tuple contains `(exact_supply_amount, minimum_target_amount)`.
	ExactSupply(Balance, Balance),
	/// Swaps a maximum amount of the supply currency for an exact amount of the target currency.
	/// The tuple contains `(maximum_supply_amount, exact_target_amount)`.
	ExactTarget(Balance, Balance),
}

/// A trait for managing a DEX.
pub trait DEXManager<AccountId, Balance, CurrencyId> {
	/// Returns the liquidity pool for a given pair of currencies.
	fn get_liquidity_pool(
		currency_id_a: CurrencyId,
		currency_id_b: CurrencyId,
	) -> (Balance, Balance);


	/// Returns the swap amount for a given path and limit.
	fn get_swap_amount(
		path: &[CurrencyId],
		limit: SwapLimit<Balance>,
	) -> Option<(Balance, Balance)>;

	/// Adds liquidity to a currency pair.
	fn add_liquidity(
		who: &AccountId,
		currency_id_a: CurrencyId,
		currency_id_b: CurrencyId,
		max_amount_a: Balance,
		max_amount_b: Balance,
		min_share_increment: Balance,
	) -> Result<(Balance, Balance, Balance), DispatchError>;

	/// Removes liquidity from a currency pair.
	fn remove_liquidity(
		who: &AccountId,
		currency_id_a: CurrencyId,
		currency_id_b: CurrencyId,
		remove_share: Balance,
		min_withdrawn_a: Balance,
		min_withdrawn_b: Balance,
	) -> Result<(Balance, Balance), DispatchError>;
}

pub trait Swap<AccountId, Balance, CurrencyId>
where
	CurrencyId: Clone,
{
	/// Returns the swap amount for a given supply and target currency.
	fn get_swap_amount(
		supply_currency_id: CurrencyId,
		target_currency_id: CurrencyId,
		limit: SwapLimit<Balance>,
	) -> Option<(Balance, Balance)>;

	/// Swaps a supply currency for a target currency.
	fn swap(
		who: &AccountId,
		supply_currency_id: CurrencyId,
		target_currency_id: CurrencyId,
		limit: SwapLimit<Balance>,
	) -> Result<(Balance, Balance), DispatchError>;

	/// Swaps currencies along a given path.
	fn swap_by_path(
		who: &AccountId,
		swap_path: &[CurrencyId],
		limit: SwapLimit<Balance>,
	) -> Result<(Balance, Balance), DispatchError>;
}

#[cfg(feature = "std")]
impl<AccountId, CurrencyId, Balance> DEXManager<AccountId, Balance, CurrencyId> for ()
where
	Balance: Default,
{
	fn get_liquidity_pool(
		_currency_id_a: CurrencyId,
		_currency_id_b: CurrencyId,
	) -> (Balance, Balance) {
		Default::default()
	}

	fn get_swap_amount(
		_path: &[CurrencyId],
		_limit: SwapLimit<Balance>,
	) -> Option<(Balance, Balance)> {
		Some(Default::default())
	}

	fn add_liquidity(
		_who: &AccountId,
		_currency_id_a: CurrencyId,
		_currency_id_b: CurrencyId,
		_max_amount_a: Balance,
		_max_amount_b: Balance,
		_min_share_increment: Balance,
	) -> Result<(Balance, Balance, Balance), DispatchError> {
		Ok(Default::default())
	}

	fn remove_liquidity(
		_who: &AccountId,
		_currency_id_a: CurrencyId,
		_currency_id_b: CurrencyId,
		_remove_share: Balance,
		_min_withdrawn_a: Balance,
		_min_withdrawn_b: Balance,
	) -> Result<(Balance, Balance), DispatchError> {
		Ok(Default::default())
	}
}
