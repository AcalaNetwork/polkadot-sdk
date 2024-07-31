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

//! Benchmarks for the foreign assets migration.

use frame_benchmarking::v2::*;
use frame_support::{migrations::SteppedMigration, weights::WeightMeter};
use pallet_assets::{Config, Asset};
use xcm::{v3, v4};

use crate::Pallet;
use super::{old, Migration, mock_asset_details};

#[benchmarks]
mod benches {
    use super::*;

    #[benchmark]
    fn step() {
        let key = v3::Location::new(1, [v3::Junction::Parachain(2004)]);
        let mock_asset_details = mock_asset_details();
        dbg!(&std::any::type_name::<<T as pallet_assets::Config>::AssetId>());
        // old::Asset::<T, ()>::insert(key.clone(), mock_asset_details);

        let mut meter = WeightMeter::new();
        #[block]
        {
            Migration::<T, ()>::step(None, &mut meter).unwrap();
        }

        // let new_key = v4::Location::new(1, [v4::Junction::Parachain(2004)]);
        // assert!(Asset::<T>::contains_key(new_key));
    }

    impl_benchmark_test_suite!(Pallet, crate::v1::tests::new_test_ext(), crate::v1::tests::Runtime);
}
