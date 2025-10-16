// This file is part of Substrate.
//
// Copyright (C) 2020-2025 Acala Foundation.
// SPDX-License-Identifier: Apache-2.0
//
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

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as Savings;
use frame_benchmarking::{v2::*, BenchmarkError};
use frame_support::{
	assert_ok,
	traits::{EnsureOrigin, Get, Hooks},
};
use frame_system::{pallet_prelude::BlockNumberFor, Pallet as System};
use sp_runtime::traits::{AccountIdConversion, Saturating};

/// Helper trait for preparing benchmarking state.
pub trait BenchmarkHelper<AccountId, AssetId, Balance> {
	/// Prepare the environment for the pool identified by `pool_index`.
	///
	/// The implementation should make sure the provided assets exist and that the savings
	/// account holds at least `reward_budget` units of the reward asset so the benchmark can
	/// execute the transfer during `on_initialize`.
	fn prepare_pool(
		pool_index: u32,
		savings_account: &AccountId,
		reward_budget: Balance,
	) -> (AssetId, AssetId, Option<AccountId>);
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn create_pool() -> Result<(), BenchmarkError> {
		let origin =
			T::UpdateOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let savings_account = T::PalletId::get().into_account_truncating();
		let reward_rate_per_block = T::Balance::from(BlockNumberFor::<T>::from(1u32));

		let (staked_asset_id, reward_asset_id, admin) =
			T::BenchmarkHelper::prepare_pool(0, &savings_account, reward_rate_per_block);

		#[extrinsic_call]
		_(origin, staked_asset_id, reward_asset_id, reward_rate_per_block, admin);

		Ok(())
	}

	#[benchmark]
	fn on_initialize(p: Linear<1, { T::MaxRewardPools::get() }>) -> Result<(), BenchmarkError> {
		let savings_account = T::PalletId::get().into_account_truncating();

		RewardPools::<T>::kill();
		LastUpdatedBlock::<T>::kill();

		let last_block = BlockNumberFor::<T>::from(1u32);
		let elapsed_blocks = BlockNumberFor::<T>::from(10u32);
		let now = last_block.saturating_add(elapsed_blocks);

		let reward_rate_per_block = T::Balance::from(BlockNumberFor::<T>::from(1u32));
		let elapsed_balance = T::Balance::from(elapsed_blocks);
		let reward_budget = reward_rate_per_block.saturating_mul(elapsed_balance);

		for idx in 0..p {
			let (staked_asset_id, reward_asset_id, admin) =
				T::BenchmarkHelper::prepare_pool(idx, &savings_account, reward_budget);
			let origin =
				T::UpdateOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

			assert_ok!(Savings::<T>::create_pool(
				origin,
				staked_asset_id,
				reward_asset_id,
				reward_rate_per_block,
				admin,
			));
		}

		LastUpdatedBlock::<T>::put(last_block);
		System::<T>::set_block_number(now);

		#[block]
		{
			Savings::<T>::on_initialize(now);
		}

		Ok(())
	}

	impl_benchmark_test_suite!(Savings, crate::mock::new_test_ext(), crate::mock::Test);
}
