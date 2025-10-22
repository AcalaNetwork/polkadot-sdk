// This file is part of Substrate.

// Copyright (C) 2020-2025 Acala Foundation.
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

use super::*;
use crate::Pallet as CDPTreasuryPallet;
use frame_support::traits::honzon::CDPTreasury;

use frame_benchmarking::v2::*;

use frame_support::assert_ok;
use frame_system::RawOrigin;

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn extract_surplus_to_treasury() {
		let surplus_amount = T::Balance::from(1_000u32);
		assert_ok!(CDPTreasuryPallet::<T>::on_system_surplus(T::Balance::from(1_000u32)));
		assert_eq!(CDPTreasuryPallet::<T>::get_surplus_pool(), T::Balance::from(1_000u32));

		#[extrinsic_call]
		_(RawOrigin::Root, surplus_amount);
		assert_eq!(CDPTreasuryPallet::<T>::get_surplus_pool(), T::Balance::zero());
	}

	#[benchmark]
	fn auction_collateral(b: Linear<1, { T::MaxAuctionsCount::get() as u32 }>) {
		// amount div auctions_count
		let auction_size = (1_000u32) / b as u32;
		assert_ok!(CDPTreasuryPallet::<T>::set_expected_collateral_auction_size(
			RawOrigin::Root.into(),
			T::Balance::from(auction_size)
		));
		// Mint enough collaterals
		assert_ok!(T::Fungibles::mint_into(
			T::CollateralCurrencyId::get(),
			&CDPTreasuryPallet::<T>::account_id(),
			T::Balance::from(10_000u32)
		));

		let collateral_amount = T::Balance::from(1_000u32);
		let target_amount = T::Balance::from(1_000u32);
		#[extrinsic_call]
		_(RawOrigin::Root, collateral_amount, target_amount, true);

		// TODO: Modify MockAuctionManagerHandler to make it pass
		// assert_eq!(CDPTreasuryPallet::total_collaterals_not_in_auction(), 9_000);
	}

	#[benchmark]
	fn set_expected_collateral_auction_size() {
		#[extrinsic_call]
		_(RawOrigin::Root, T::Balance::from(200u32));
		assert_eq!(
			CDPTreasuryPallet::<T>::expected_collateral_auction_size(),
			T::Balance::from(200u32)
		);
	}

	#[benchmark]
	fn set_debit_offset_buffer() {
		#[extrinsic_call]
		_(RawOrigin::Root, T::Balance::from(200u32));
		assert_eq!(CDPTreasuryPallet::<T>::debit_offset_buffer(), T::Balance::from(200u32));
	}

	impl_benchmark_test_suite! {
		CDPTreasuryPallet,
		crate::mock::ExtBuilder::default().build(),
		crate::mock::Test,
	}
}
