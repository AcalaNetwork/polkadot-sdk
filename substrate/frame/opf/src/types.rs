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

//! Types & Imports for Distribution pallet.

pub use super::*;

pub use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible,
		fungible::{Inspect, Mutate, MutateHold},
		fungibles,
		schedule::{
			v3::{Anon as ScheduleAnon, Named as ScheduleNamed},
			DispatchTime, MaybeHashed,
		},
		tokens::{Precision, Preservation},
		Bounded, DefensiveOption, EnsureOrigin, LockIdentifier, OriginTrait, QueryPreimage,
		StorePreimage,
	},
	transactional,
	weights::WeightMeter,
	PalletId, Serialize,
};
pub use frame_system::{pallet_prelude::*, RawOrigin};
pub use pallet_conviction_voting::Conviction;
pub use scale_info::prelude::vec::Vec;
pub use sp_runtime::{
	traits::{
		AccountIdConversion, BlockNumberProvider, Convert, Dispatchable, Saturating, StaticLookup,
		Zero,
	},
	Percent,
};
pub use sp_std::boxed::Box;

pub type BalanceOf<T> = <<T as Config>::NativeBalance as fungible::Inspect<
	<T as frame_system::Config>::AccountId,
>>::Balance;
pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
/// A reward index.
pub type SpendIndex = u32;
pub type CallOf<T> = <T as frame_system::Config>::RuntimeCall;
pub type BoundedCallOf<T> = Bounded<CallOf<T>, <T as frame_system::Config>::Hashing>;
pub type ProjectId<T> = AccountIdOf<T>;
pub type PalletsOriginOf<T> =
	<<T as frame_system::Config>::RuntimeOrigin as OriginTrait>::PalletsOrigin;
pub const DISTRIBUTION_ID: LockIdentifier = *b"distribu";
pub type RoundIndex = u32;
pub type VoterId<T> = AccountIdOf<T>;
pub type ProvidedBlockNumberFor<T> =
	<<T as Config>::BlockNumberProvider as BlockNumberProvider>::BlockNumber;
pub use frame_system::pallet_prelude::BlockNumberFor as SystemBlockNumberFor;

/// The state of the payment claim.
#[derive(Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen, RuntimeDebug, TypeInfo, Default)]
pub enum SpendState {
	/// Unclaimed
	#[default]
	Unclaimed,
	/// Claimed & Paid.
	Completed,
	/// Claimed but Failed.
	Failed,
}

//Processed Reward status
#[derive(Encode, Decode, Clone, PartialEq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct SpendInfo<T: Config> {
	/// The asset amount of the spend.
	pub amount: BalanceOf<T>,
	/// The block number from which the spend can be claimed(24h after SpendStatus Creation).
	pub valid_from: ProvidedBlockNumberFor<T>,
	/// Corresponding project id
	pub whitelisted_project: ProjectInfo<T>,
	/// Has it been claimed?
	pub claimed: bool,
	/// Claim Expiration block
	pub expire: ProvidedBlockNumberFor<T>,
}

impl<T: Config> SpendInfo<T> {
	pub fn new(whitelisted: &ProjectInfo<T>) -> Self {
		let amount = whitelisted.amount;
		let whitelisted_project = whitelisted.clone();
		let claimed = false;
		let valid_from = T::BlockNumberProvider::current_block_number();
		let expire = valid_from.saturating_add(T::ClaimingPeriod::get());

		let spend = SpendInfo { amount, valid_from, whitelisted_project, claimed, expire };

		//Add it to the Spends storage
		Spends::<T>::insert(whitelisted.project_id.clone(), spend.clone());

		spend
	}
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct ProjectInfo<T: Config> {
	/// AcountId that will receive the payment.
	pub project_id: ProjectId<T>,

	/// Block at which the project was submitted for reward distribution
	pub submission_block: ProvidedBlockNumberFor<T>,

	/// Amount to be lock & pay for this project
	pub amount: BalanceOf<T>,
}

#[derive(Encode, Decode, Clone, PartialEq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct VoteInfo<T: Config> {
	/// The amount of stake/slash placed on this vote.
	pub amount: BalanceOf<T>,

	/// Round at which the vote was casted
	pub round: VotingRoundInfo<T>,

	/// Whether the vote is "fund" / "not fund"
	pub is_fund: bool,

	pub conviction: Conviction,

	pub funds_unlock_block: ProvidedBlockNumberFor<T>,
}

// If no conviction, user's funds are released at the end of the voting round
impl<T: Config> VoteInfo<T> {
	pub fn funds_unlock(&mut self) {
		let conviction_coeff = <u8 as From<Conviction>>::from(self.conviction);
		let funds_unlock_block = self.round.round_ending_block;
		self.funds_unlock_block = funds_unlock_block;
	}
}

impl<T: Config> Default for VoteInfo<T> {
	// Dummy vote infos used to handle errors
	fn default() -> Self {
		// get round number
		let round = VotingRounds::<T>::get(0).expect("Round 0 exists");
		let amount = Zero::zero();
		let is_fund = false;
		let conviction = Conviction::None;
		let funds_unlock_block = round.round_ending_block;
		VoteInfo { amount, round, is_fund, conviction, funds_unlock_block }
	}
}

/// Voting rounds are periodically created inside a hook on_initialize (use poll in the future)
#[derive(Encode, Decode, Clone, PartialEq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct VotingRoundInfo<T: Config> {
	pub round_number: u32,
	pub round_starting_block: ProvidedBlockNumberFor<T>,
	pub round_ending_block: ProvidedBlockNumberFor<T>,
	pub total_positive_votes_amount: BalanceOf<T>,
	pub total_negative_votes_amount: BalanceOf<T>,
}

impl<T: Config> VotingRoundInfo<T> {
	pub fn new() -> Self {
		let round_starting_block = T::BlockNumberProvider::current_block_number();
		let round_ending_block = round_starting_block
			.clone()
			.checked_add(&T::VotingPeriod::get())
			.expect("Invalid Result");
		let round_number = VotingRoundNumber::<T>::get();
		let new_number = round_number.checked_add(1).expect("Invalid Result");
		VotingRoundNumber::<T>::put(new_number);
		let total_positive_votes_amount = BalanceOf::<T>::zero();
		let total_negative_votes_amount = BalanceOf::<T>::zero();

		Pallet::<T>::deposit_event(Event::<T>::VotingRoundStarted {
			when: round_starting_block,
			round_number,
		});

		let round_infos = VotingRoundInfo {
			round_number,
			round_starting_block,
			round_ending_block,
			total_positive_votes_amount,
			total_negative_votes_amount,
		};
		VotingRounds::<T>::insert(round_number, round_infos.clone());
		round_infos
	}
}
