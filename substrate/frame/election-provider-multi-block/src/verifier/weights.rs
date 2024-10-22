// This file is part of Substrate.

// Copyright (C) 2022 Parity Technologies (UK) Ltd.
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

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::Weight};
use core::marker::PhantomData;

pub trait WeightInfo {
	fn on_initialize_ongoing(v: u32, t: u32) -> Weight;
	fn on_initialize_ongoing_failed(v: u32, t: u32) -> Weight;
	fn on_initialize_ongoing_finalize(v: u32, t: u32) -> Weight;
	fn on_initialize_ongoing_finalize_failed(v: u32, t: u32) -> Weight;
	fn finalize_async_verification(v: u32, t: u32, ) -> Weight;
	fn verify_sync_paged(v: u32, t: u32, ) -> Weight;
}

/// Weight functions for `pallet_epm_verifier`.
pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	/// Storage: `ElectionVerifierPallet::VerificationStatus` (r:1 w:1)
	/// Proof: `ElectionVerifierPallet::VerificationStatus` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionProviderMultiBlock::Round` (r:1 w:0)
	/// Proof: `ElectionProviderMultiBlock::Round` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionSignedPallet::SortedScores` (r:1 w:0)
	/// Proof: `ElectionSignedPallet::SortedScores` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionSignedPallet::SubmissionStorage` (r:1 w:0)
	/// Proof: `ElectionSignedPallet::SubmissionStorage` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionProviderMultiBlock::PagedTargetSnapshot` (r:1 w:0)
	/// Proof: `ElectionProviderMultiBlock::PagedTargetSnapshot` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionProviderMultiBlock::PagedVoterSnapshot` (r:1 w:0)
	/// Proof: `ElectionProviderMultiBlock::PagedVoterSnapshot` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Staking::ValidatorCount` (r:1 w:0)
	/// Proof: `Staking::ValidatorCount` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ElectionVerifierPallet::QueuedValidVariant` (r:1 w:0)
	/// Proof: `ElectionVerifierPallet::QueuedValidVariant` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionVerifierPallet::QueuedSolutionY` (r:0 w:1)
	/// Proof: `ElectionVerifierPallet::QueuedSolutionY` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionVerifierPallet::LastStoredPage` (r:0 w:1)
	/// Proof: `ElectionVerifierPallet::LastStoredPage` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionVerifierPallet::QueuedSolutionBackings` (r:0 w:1)
	/// Proof: `ElectionVerifierPallet::QueuedSolutionBackings` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// The range of component `v` is `[32, 1024]`.
	/// The range of component `t` is `[512, 2048]`.
	fn on_initialize_ongoing(v: u32, t: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `12992 + t * (26 ±0) + v * (80 ±0)`
		//  Estimated: `15414 + t * (27 ±1) + v * (80 ±2)`
		// Minimum execution time: 2_036_000_000 picoseconds.
		Weight::from_parts(2_036_000_000, 0)
			.saturating_add(Weight::from_parts(0, 15414))
			// Standard Error: 3_307_370
			.saturating_add(Weight::from_parts(20_614_626, 0).saturating_mul(v.into()))
			// Standard Error: 1_618_727
			.saturating_add(Weight::from_parts(1_324_037, 0).saturating_mul(t.into()))
			.saturating_add(T::DbWeight::get().reads(8))
			.saturating_add(T::DbWeight::get().writes(4))
			.saturating_add(Weight::from_parts(0, 27).saturating_mul(t.into()))
			.saturating_add(Weight::from_parts(0, 80).saturating_mul(v.into()))
	}
	/// Storage: `ElectionVerifierPallet::VerificationStatus` (r:1 w:1)
	/// Proof: `ElectionVerifierPallet::VerificationStatus` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionProviderMultiBlock::Round` (r:1 w:0)
	/// Proof: `ElectionProviderMultiBlock::Round` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionSignedPallet::SortedScores` (r:1 w:1)
	/// Proof: `ElectionSignedPallet::SortedScores` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionSignedPallet::SubmissionStorage` (r:1 w:1)
	/// Proof: `ElectionSignedPallet::SubmissionStorage` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionProviderMultiBlock::PagedTargetSnapshot` (r:1 w:0)
	/// Proof: `ElectionProviderMultiBlock::PagedTargetSnapshot` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionProviderMultiBlock::PagedVoterSnapshot` (r:1 w:0)
	/// Proof: `ElectionProviderMultiBlock::PagedVoterSnapshot` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionVerifierPallet::QueuedValidVariant` (r:1 w:0)
	/// Proof: `ElectionVerifierPallet::QueuedValidVariant` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionSignedPallet::SubmissionMetadataStorage` (r:1 w:1)
	/// Proof: `ElectionSignedPallet::SubmissionMetadataStorage` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionProviderMultiBlock::CurrentPhase` (r:1 w:0)
	/// Proof: `ElectionProviderMultiBlock::CurrentPhase` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// The range of component `v` is `[32, 1024]`.
	/// The range of component `t` is `[512, 2048]`.
	fn on_initialize_ongoing_failed(v: u32, _t: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0 + t * (4 ±0) + v * (112 ±0)`
		//  Estimated: `7604 + v * (108 ±2)`
		// Minimum execution time: 1_034_000_000 picoseconds.
		Weight::from_parts(1_576_541_397, 0)
			.saturating_add(Weight::from_parts(0, 7604))
			// Standard Error: 296_982
			.saturating_add(Weight::from_parts(3_076_310, 0).saturating_mul(v.into()))
			.saturating_add(T::DbWeight::get().reads(9))
			.saturating_add(T::DbWeight::get().writes(4))
			.saturating_add(Weight::from_parts(0, 108).saturating_mul(v.into()))
	}
	/// Storage: `ElectionVerifierPallet::VerificationStatus` (r:1 w:1)
	/// Proof: `ElectionVerifierPallet::VerificationStatus` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionProviderMultiBlock::Round` (r:1 w:0)
	/// Proof: `ElectionProviderMultiBlock::Round` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionSignedPallet::SortedScores` (r:1 w:0)
	/// Proof: `ElectionSignedPallet::SortedScores` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionSignedPallet::SubmissionStorage` (r:1 w:0)
	/// Proof: `ElectionSignedPallet::SubmissionStorage` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionProviderMultiBlock::PagedTargetSnapshot` (r:1 w:0)
	/// Proof: `ElectionProviderMultiBlock::PagedTargetSnapshot` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionProviderMultiBlock::PagedVoterSnapshot` (r:1 w:0)
	/// Proof: `ElectionProviderMultiBlock::PagedVoterSnapshot` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Staking::ValidatorCount` (r:1 w:0)
	/// Proof: `Staking::ValidatorCount` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ElectionVerifierPallet::QueuedValidVariant` (r:1 w:1)
	/// Proof: `ElectionVerifierPallet::QueuedValidVariant` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionVerifierPallet::QueuedSolutionBackings` (r:3 w:1)
	/// Proof: `ElectionVerifierPallet::QueuedSolutionBackings` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionVerifierPallet::QueuedSolutionScore` (r:1 w:1)
	/// Proof: `ElectionVerifierPallet::QueuedSolutionScore` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionVerifierPallet::QueuedSolutionY` (r:0 w:1)
	/// Proof: `ElectionVerifierPallet::QueuedSolutionY` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionVerifierPallet::LastStoredPage` (r:0 w:1)
	/// Proof: `ElectionVerifierPallet::LastStoredPage` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// The range of component `v` is `[32, 1024]`.
	/// The range of component `t` is `[512, 2048]`.
	fn on_initialize_ongoing_finalize(v: u32, t: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0 + t * (41 ±0) + v * (125 ±0)`
		//  Estimated: `79043 + t * (10 ±8) + v * (85 ±17)`
		// Minimum execution time: 1_724_000_000 picoseconds.
		Weight::from_parts(1_466_010_752, 0)
			.saturating_add(Weight::from_parts(0, 79043))
			// Standard Error: 199_409
			.saturating_add(Weight::from_parts(3_322_580, 0).saturating_mul(v.into()))
			// Standard Error: 128_785
			.saturating_add(Weight::from_parts(128_906, 0).saturating_mul(t.into()))
			.saturating_add(T::DbWeight::get().reads(12))
			.saturating_add(T::DbWeight::get().writes(6))
			.saturating_add(Weight::from_parts(0, 10).saturating_mul(t.into()))
			.saturating_add(Weight::from_parts(0, 85).saturating_mul(v.into()))
	}
	/// The range of component `v` is `[32, 1024]`.
	/// The range of component `t` is `[512, 2048]`.
	fn on_initialize_ongoing_finalize_failed(_v: u32, _t: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 3_000_000 picoseconds.
		Weight::from_parts(4_659_677, 0)
			.saturating_add(Weight::from_parts(0, 0))
	}
	/// The range of component `v` is `[32, 1024]`.
	/// The range of component `t` is `[512, 2048]`.
	fn finalize_async_verification(v: u32, t: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `0`
		//  Estimated: `0`
		// Minimum execution time: 3_000_000 picoseconds.
		Weight::from_parts(3_354_301, 0)
			.saturating_add(Weight::from_parts(0, 0))
			// Standard Error: 2_197
			.saturating_add(Weight::from_parts(907, 0).saturating_mul(v.into()))
			// Standard Error: 1_419
			.saturating_add(Weight::from_parts(65, 0).saturating_mul(t.into()))
	}
	/// Storage: `ElectionVerifierPallet::QueuedSolutionScore` (r:1 w:0)
	/// Proof: `ElectionVerifierPallet::QueuedSolutionScore` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionVerifierPallet::MinimumScore` (r:1 w:0)
	/// Proof: `ElectionVerifierPallet::MinimumScore` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionProviderMultiBlock::PagedTargetSnapshot` (r:1 w:0)
	/// Proof: `ElectionProviderMultiBlock::PagedTargetSnapshot` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionProviderMultiBlock::PagedVoterSnapshot` (r:1 w:0)
	/// Proof: `ElectionProviderMultiBlock::PagedVoterSnapshot` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `Staking::ValidatorCount` (r:1 w:0)
	/// Proof: `Staking::ValidatorCount` (`max_values`: Some(1), `max_size`: Some(4), added: 499, mode: `MaxEncodedLen`)
	/// Storage: `ElectionVerifierPallet::QueuedValidVariant` (r:1 w:0)
	/// Proof: `ElectionVerifierPallet::QueuedValidVariant` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionVerifierPallet::QueuedSolutionY` (r:0 w:1)
	/// Proof: `ElectionVerifierPallet::QueuedSolutionY` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionVerifierPallet::LastStoredPage` (r:0 w:1)
	/// Proof: `ElectionVerifierPallet::LastStoredPage` (`max_values`: Some(1), `max_size`: None, mode: `Measured`)
	/// Storage: `ElectionVerifierPallet::QueuedSolutionBackings` (r:0 w:1)
	/// Proof: `ElectionVerifierPallet::QueuedSolutionBackings` (`max_values`: None, `max_size`: None, mode: `Measured`)
	/// The range of component `v` is `[32, 1024]`.
	/// The range of component `t` is `[512, 2048]`.
	fn verify_sync_paged(v: u32, t: u32, ) -> Weight {
		// Proof Size summary in bytes:
		//  Measured:  `15968 + t * (24 ±0) + v * (73 ±0)`
		//  Estimated: `18127 + t * (25 ±2) + v * (72 ±3)`
		// Minimum execution time: 1_403_000_000 picoseconds.
		Weight::from_parts(1_403_000_000, 0)
			.saturating_add(Weight::from_parts(0, 18127))
			// Standard Error: 3_979_877
			.saturating_add(Weight::from_parts(24_084_766, 0).saturating_mul(v.into()))
			// Standard Error: 1_947_873
			.saturating_add(Weight::from_parts(1_727_080, 0).saturating_mul(t.into()))
			.saturating_add(T::DbWeight::get().reads(6))
			.saturating_add(T::DbWeight::get().writes(3))
			.saturating_add(Weight::from_parts(0, 25).saturating_mul(t.into()))
			.saturating_add(Weight::from_parts(0, 72).saturating_mul(v.into()))
	}
}

impl WeightInfo for () {
	fn on_initialize_ongoing(_v: u32, _t: u32) -> Weight {
		Default::default()
	}

	fn on_initialize_ongoing_failed(_v: u32, _t: u32) -> Weight {
		Default::default()
	}

	fn on_initialize_ongoing_finalize(_v: u32, _t: u32) -> Weight {
		Default::default()
	}

	fn on_initialize_ongoing_finalize_failed(_v: u32, _t: u32) -> Weight {
		Default::default()
	}

	fn finalize_async_verification(_v: u32, _t: u32, ) -> Weight {
		Default::default()
	}

	fn verify_sync_paged(_v: u32, _t: u32, ) -> Weight {
		Default::default()
	}
}

