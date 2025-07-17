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

//! Contains the fee types that need to be configured for `pallet-transaction-payment`.

use crate::{BalanceOf, Config};
use core::marker::PhantomData;
use frame_support::{pallet_prelude::Weight, traits::Get, weights::WeightToFee};
use sp_runtime::{FixedPointNumber, FixedU128, SaturatedConversion, Saturating};

/// The only [`WeightToFee`] implementation that is supported by this pallet.
///
/// `P,Q`: Rational number that defines the ref_time to fee mapping.
///
/// This enforces a ration of ref_time and proof_time that is proportional
/// to their distribution in the block limits. We enforce the usage of this fee
/// structure because our gas mapping depends on it.
///
/// # Panics
///
/// If either `P` or `Q` is zero.
pub struct BlockRatioFee<const P: u128, const Q: u128, T: Config>(PhantomData<T>);

/// A that signals that [`BlockRatioFee`] is used by the runtime.
///
/// This trait is sealed. Use [`BlockRatioFee`].
pub trait BlockRatioWeightToFee: seal::Sealed {
	/// The runtime.
	type T: Config;
	/// The ref_time to fee coefficient.
	const REF_TIME_TO_FEE: FixedU128;

	/// The proof_size to fee coefficient.
	fn proof_size_to_fee() -> FixedU128 {
		let max_weight = <Self::T as frame_system::Config>::BlockWeights::get().max_block;
		let ratio =
			FixedU128::from_rational(max_weight.ref_time().into(), max_weight.proof_size().into());
		Self::REF_TIME_TO_FEE.saturating_mul(ratio)
	}

	/// Convert a fee back to a weight.
	fn fee_to_weight(fee: u64) -> Weight {
		Weight::from_parts(
			Self::REF_TIME_TO_FEE
				.reciprocal()
				.expect("Fees are not allowed to be zero.")
				.saturating_mul_int(fee),
			Self::proof_size_to_fee()
				.reciprocal()
				.expect("Fees are not allowed to be zero.")
				.saturating_mul_int(fee),
		)
	}
}

impl<const P: u128, const Q: u128, T: Config> BlockRatioWeightToFee for BlockRatioFee<P, Q, T> {
	type T = T;
	const REF_TIME_TO_FEE: FixedU128 = FixedU128::from_rational(P, Q);
}

impl<const P: u128, const Q: u128, T: Config> WeightToFee for BlockRatioFee<P, Q, T> {
	type Balance = BalanceOf<T>;

	fn weight_to_fee(weight: &Weight) -> Self::Balance {
		let ref_time_fee = Self::REF_TIME_TO_FEE
			.saturating_mul_int(Self::Balance::saturated_from(weight.ref_time()));
		let proof_size_fee = Self::proof_size_to_fee()
			.saturating_mul_int(Self::Balance::saturated_from(weight.proof_size()));
		ref_time_fee.max(proof_size_fee)
	}
}

mod seal {
	pub trait Sealed {}
	impl<const P: u128, const Q: u128, T: super::Config> Sealed for super::BlockRatioFee<P, Q, T> {}
}
