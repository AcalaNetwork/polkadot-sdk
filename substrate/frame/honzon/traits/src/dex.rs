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

/// Represents a swap path that can be aggregated across different DEX protocols.
#[derive(Encode, Decode, Eq, PartialEq, Clone, RuntimeDebug, PartialOrd, Ord, TypeInfo)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum AggregatedSwapPath<CurrencyId, StableAssetPoolId, PoolTokenIndex> {
	/// A swap path through a traditional DEX.
	Dex(Vec<CurrencyId>),
	/// A swap path through a Taiga stable asset pool.
	Taiga(StableAssetPoolId, PoolTokenIndex, PoolTokenIndex),
}

/// A trait for managing a DEX.
pub trait DEXManager<AccountId, Balance, CurrencyId> {
	/// Returns the liquidity pool for a given pair of currencies.
	fn get_liquidity_pool(
		currency_id_a: CurrencyId,
		currency_id_b: CurrencyId,
	) -> (Balance, Balance);

	/// Returns the address of the liquidity token for a given pair of currencies.
	fn get_liquidity_token_address(
		currency_id_a: CurrencyId,
		currency_id_b: CurrencyId,
	) -> Option<H160>;

	/// Returns the swap amount for a given path and limit.
	fn get_swap_amount(
		path: &[CurrencyId],
		limit: SwapLimit<Balance>,
	) -> Option<(Balance, Balance)>;

	/// Returns the best swap path for a given supply and target currency.
	fn get_best_price_swap_path(
		supply_currency_id: CurrencyId,
		target_currency_id: CurrencyId,
		limit: SwapLimit<Balance>,
		alternative_path_joint_list: Vec<Vec<CurrencyId>>,
	) -> Option<(Vec<CurrencyId>, Balance, Balance)>;

	/// Swaps currencies along a specific path.
	fn swap_with_specific_path(
		who: &AccountId,
		path: &[CurrencyId],
		limit: SwapLimit<Balance>,
	) -> Result<(Balance, Balance), DispatchError>;

	/// Adds liquidity to a currency pair.
	fn add_liquidity(
		who: &AccountId,
		currency_id_a: CurrencyId,
		currency_id_b: CurrencyId,
		max_amount_a: Balance,
		max_amount_b: Balance,
		min_share_increment: Balance,
		stake_increment_share: bool,
	) -> Result<(Balance, Balance, Balance), DispatchError>;

	/// Removes liquidity from a currency pair.
	fn remove_liquidity(
		who: &AccountId,
		currency_id_a: CurrencyId,
		currency_id_b: CurrencyId,
		remove_share: Balance,
		min_withdrawn_a: Balance,
		min_withdrawn_b: Balance,
		by_unstake: bool,
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

	/// Swaps currencies along a given aggregated path.
	fn swap_by_aggregated_path<StableAssetPoolId, PoolTokenIndex>(
		who: &AccountId,
		swap_path: &[AggregatedSwapPath<CurrencyId, StableAssetPoolId, PoolTokenIndex>],
		limit: SwapLimit<Balance>,
	) -> Result<(Balance, Balance), DispatchError>;
}

/// An error that can occur during a swap.
#[derive(Eq, PartialEq, RuntimeDebug)]
pub enum SwapError {
	/// The swap cannot be completed.
	CannotSwap,
}

impl Into<DispatchError> for SwapError {
	fn into(self) -> DispatchError {
		DispatchError::Other("Cannot swap")
	}
}

/// A wrapper that implements the `Swap` trait for a DEX with specific joints.
pub struct SpecificJointsSwap<Dex, Joints>(sp_std::marker::PhantomData<(Dex, Joints)>);

impl<AccountId, Balance, CurrencyId, Dex, Joints> Swap<AccountId, Balance, CurrencyId>
	for SpecificJointsSwap<Dex, Joints>
where
	Dex: DEXManager<AccountId, Balance, CurrencyId>,
	Joints: Get<Vec<Vec<CurrencyId>>>,
	Balance: Clone,
	CurrencyId: Clone,
{
	fn get_swap_amount(
		supply_currency_id: CurrencyId,
		target_currency_id: CurrencyId,
		limit: SwapLimit<Balance>,
	) -> Option<(Balance, Balance)> {
		<Dex as DEXManager<AccountId, Balance, CurrencyId>>::get_best_price_swap_path(
			supply_currency_id,
			target_currency_id,
			limit,
			Joints::get(),
		)
		.map(|(_, supply_amount, target_amount)| (supply_amount, target_amount))
	}

	fn swap(
		who: &AccountId,
		supply_currency_id: CurrencyId,
		target_currency_id: CurrencyId,
		limit: SwapLimit<Balance>,
	) -> sp_std::result::Result<(Balance, Balance), DispatchError> {
		let path = <Dex as DEXManager<AccountId, Balance, CurrencyId>>::get_best_price_swap_path(
			supply_currency_id,
			target_currency_id,
			limit.clone(),
			Joints::get(),
		)
		.ok_or_else(|| Into::<DispatchError>::into(SwapError::CannotSwap))?
		.0;

		<Dex as DEXManager<AccountId, Balance, CurrencyId>>::swap_with_specific_path(
			who, &path, limit,
		)
	}

	fn swap_by_path(
		who: &AccountId,
		swap_path: &[CurrencyId],
		limit: SwapLimit<Balance>,
	) -> Result<(Balance, Balance), DispatchError> {
		<Dex as DEXManager<AccountId, Balance, CurrencyId>>::swap_with_specific_path(
			who, swap_path, limit,
		)
	}

	fn swap_by_aggregated_path<StableAssetPoolId, PoolTokenIndex>(
		who: &AccountId,
		swap_path: &[AggregatedSwapPath<CurrencyId, StableAssetPoolId, PoolTokenIndex>],
		limit: SwapLimit<Balance>,
	) -> Result<(Balance, Balance), DispatchError> {
		ensure!(swap_path.len() == 1, Into::<DispatchError>::into(SwapError::CannotSwap));
		match swap_path.last() {
			Some(AggregatedSwapPath::<CurrencyId, StableAssetPoolId, PoolTokenIndex>::Dex(
				path,
			)) => <Dex as DEXManager<AccountId, Balance, CurrencyId>>::swap_with_specific_path(
				who, path, limit,
			),
			_ => Err(Into::<DispatchError>::into(SwapError::CannotSwap)),
		}
	}
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

	fn get_liquidity_token_address(
		_currency_id_a: CurrencyId,
		_currency_id_b: CurrencyId,
	) -> Option<H160> {
		Some(Default::default())
	}

	fn get_swap_amount(
		_path: &[CurrencyId],
		_limit: SwapLimit<Balance>,
	) -> Option<(Balance, Balance)> {
		Some(Default::default())
	}

	fn get_best_price_swap_path(
		_supply_currency_id: CurrencyId,
		_target_currency_id: CurrencyId,
		_limit: SwapLimit<Balance>,
		_alternative_path_joint_list: Vec<Vec<CurrencyId>>,
	) -> Option<(Vec<CurrencyId>, Balance, Balance)> {
		Some(Default::default())
	}

	fn swap_with_specific_path(
		_who: &AccountId,
		_path: &[CurrencyId],
		_limit: SwapLimit<Balance>,
	) -> Result<(Balance, Balance), DispatchError> {
		Ok(Default::default())
	}

	fn add_liquidity(
		_who: &AccountId,
		_currency_id_a: CurrencyId,
		_currency_id_b: CurrencyId,
		_max_amount_a: Balance,
		_max_amount_b: Balance,
		_min_share_increment: Balance,
		_stake_increment_share: bool,
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
		_by_unstake: bool,
	) -> Result<(Balance, Balance), DispatchError> {
		Ok(Default::default())
	}
}

/// A trait for bootstrapping a DEX.
pub trait DEXBootstrap<AccountId, Balance, CurrencyId> {
	/// Returns the provision pool for a given currency pair.
	fn get_provision_pool(
		currency_id_a: CurrencyId,
		currency_id_b: CurrencyId,
	) -> (Balance, Balance);

	/// Returns the provision pool of a specific account for a given currency pair.
	fn get_provision_pool_of(
		who: &AccountId,
		currency_id_a: CurrencyId,
		currency_id_b: CurrencyId,
	) -> (Balance, Balance);

	/// Returns the initial share exchange rate for a given currency pair.
	fn get_initial_share_exchange_rate(
		currency_id_a: CurrencyId,
		currency_id_b: CurrencyId,
	) -> (Balance, Balance);

	/// Adds a provision to the DEX.
	fn add_provision(
		who: &AccountId,
		currency_id_a: CurrencyId,
		currency_id_b: CurrencyId,
		contribution_a: Balance,
		contribution_b: Balance,
	) -> DispatchResult;

	/// Claims a DEX share.
	fn claim_dex_share(
		who: &AccountId,
		currency_id_a: CurrencyId,
		currency_id_b: CurrencyId,
	) -> Result<Balance, DispatchError>;

	/// Refunds a provision.
	fn refund_provision(
		who: &AccountId,
		currency_id_a: CurrencyId,
		currency_id_b: CurrencyId,
	) -> DispatchResult;
}

#[cfg(feature = "std")]
impl<AccountId, CurrencyId, Balance> DEXBootstrap<AccountId, Balance, CurrencyId> for ()
where
	Balance: Default,
{
	fn get_provision_pool(
		_currency_id_a: CurrencyId,
		_currency_id_b: CurrencyId,
	) -> (Balance, Balance) {
		Default::default()
	}

	fn get_provision_pool_of(
		_who: &AccountId,
		_currency_id_a: CurrencyId,
		_currency_id_b: CurrencyId,
	) -> (Balance, Balance) {
		Default::default()
	}

	fn get_initial_share_exchange_rate(
		_currency_id_a: CurrencyId,
		_currency_id_b: CurrencyId,
	) -> (Balance, Balance) {
		Default::default()
	}

	fn add_provision(
		_who: &AccountId,
		_currency_id_a: CurrencyId,
		_currency_id_b: CurrencyId,
		_contribution_a: Balance,
		_contribution_b: Balance,
	) -> DispatchResult {
		Ok(())
	}

	fn claim_dex_share(
		_who: &AccountId,
		_currency_id_a: CurrencyId,
		_currency_id_b: CurrencyId,
	) -> Result<Balance, DispatchError> {
		Ok(Default::default())
	}

	fn refund_provision(
		_who: &AccountId,
		_currency_id_a: CurrencyId,
		_currency_id_b: CurrencyId,
	) -> DispatchResult {
		Ok(())
	}
}
