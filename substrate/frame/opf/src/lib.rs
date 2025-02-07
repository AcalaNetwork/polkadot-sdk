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

//! # OPF pallet.
//!
//! The OPF Pallet handles the Optimistic Project Funding.
//! It allows users to nominate projects (whitelisted in OpenGov) with their DOT.
//!
//! ## Overview
//!
//! This mechanism will be funded with a constant stream of DOT taken directly from inflation
//! and distributed to projects based on the proportion of DOT that has nominated them.
//!
//! ### Terminology
//!
//! - **MaxWhitelistedProjects:** Maximum number of Whitelisted projects that can be handled by the
//!   pallet.
//! - **VotingPeriod:**Period during which voting is enabled.
//! - **TemporaryRewards:**For test purposes only ⇒ used as a substitute for the inflation portion
//!   used for the rewards.
//! - **PotId:** Pot containing the funds used to pay the rewards.
//! - **ClaimingPeriod:**Period during which allocated funds can be claimed
//!
//! ## Interface
//!
//! ### Permissioned Calls
//! * `register_projects_batch`: Allows a SubmitOrigin to register a list of whitelisted projects
//!   for funding allocation
//! * `unregister_project`: Allows an AdminOrigin to unregister a previously whitelisted project
//!
//! ### Permissionless Calls
//! * `vote`: Allows users to [vote for/nominate] a whitelisted project using their funds.
//! * `remove_vote`: Allows users to remove a casted vote.
//! * `release_voter_funds`: Allows users to unlock funds related to a specific project.
//! * `claim_reward_for`: Claim a reward for a nominated/whitelisted project.
//! * `execute_call_dispatch`: Used for delayed calls execution

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
mod functions;
mod types;
pub use pallet_democracy as Democracy;
pub use types::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;
	use frame_system::WeightInfo;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + Democracy::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		type RuntimeCall: Parameter
			+ UnfilteredDispatchable<RuntimeOrigin = <Self as frame_system::Config>::RuntimeOrigin>
			+ From<Call<Self>>
			+ Into<<Self as frame_system::Config>::RuntimeCall>
			+ GetDispatchInfo;
		/// The admin origin that can list and un-list whitelisted projects.
		type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Type to access the Balances Pallet.
		type NativeBalance: fungible::Inspect<Self::AccountId>
			+ fungible::Mutate<Self::AccountId>
			+ fungible::hold::Inspect<Self::AccountId>
			+ fungible::hold::Mutate<Self::AccountId, Reason = Self::RuntimeHoldReason>
			+ fungible::freeze::Inspect<Self::AccountId>
			+ fungible::freeze::Mutate<Self::AccountId>;

		type RuntimeHoldReason: From<HoldReason>;
		/// Provider for the block number.
		type BlockNumberProvider: BlockNumberProvider;

		/// Treasury account Id
		#[pallet::constant]
		type PotId: Get<PalletId>;

		/// Time period in which people can vote.
		/// After the period has ended, the votes are counted (STOP THE COUNT)
		/// and then the funds are distributed into Spends.
		#[pallet::constant]
		type VotingPeriod: Get<ProvidedBlockNumberFor<Self>>;

		#[pallet::constant]
		type EnactmentPeriod: Get<ProvidedBlockNumberFor<Self>>;

		/// Maximum number projects that can be accepted by this pallet
		#[pallet::constant]
		type MaxProjects: Get<u32>;

		/// Time for claiming a Spend.
		/// After the period has passed, a spend is thrown away
		/// and the funds are available again for distribution in the pot.
		#[pallet::constant]
		type ClaimingPeriod: Get<ProvidedBlockNumberFor<Self>>;

		/// Period after which all the votes are reset.
		#[pallet::constant]
		type VoteValidityPeriod: Get<ProvidedBlockNumberFor<Self>>;

		/// Used for Pallet testing only. Represents the Total Reward distributed
		type TemporaryRewards: Get<BalanceOf<Self>>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	/// A reason for placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Funds are held for a given buffer time before payment
		#[codec(index = 0)]
		FundsReserved,
	}

	/// Number of Voting Rounds executed so far
	#[pallet::storage]
	pub type NextVotingRoundNumber<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// Returns Infos about a Voting Round agains the Voting Round index
	#[pallet::storage]
	pub type VotingRounds<T: Config> =
		StorageMap<_, Twox64Concat, RoundIndex, VotingRoundInfo<T>, OptionQuery>;

	/// Spends that still have to be claimed.
	#[pallet::storage]
	pub(super) type Spends<T: Config> =
		CountedStorageMap<_, Twox64Concat, ProjectId<T>, SpendInfo<T>, OptionQuery>;

	/// List of Whitelisted Project registered
	#[pallet::storage]
	pub type WhiteListedProjectAccounts<T: Config> =
		CountedStorageMap<_, Twox64Concat, ProjectId<T>, ProjectInfo<T>, OptionQuery>;

	/// Returns (positive_funds,negative_funds) of Whitelisted Project accounts
	#[pallet::storage]
	pub type ProjectFunds<T: Config> = StorageMap<
		_,
		Twox64Concat,
		ProjectId<T>,
		BoundedVec<BalanceOf<T>, ConstU32<2>>,
		ValueQuery,
	>;

	/// Returns Votes Infos against (project_id, voter_id) key
	#[pallet::storage]
	pub type Votes<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		ProjectId<T>,
		Twox64Concat,
		VoterId<T>,
		VoteInfo<T>,
		OptionQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Reward successfully claimed
		RewardClaimed {
			when: ProvidedBlockNumberFor<T>,
			amount: BalanceOf<T>,
			project_id: ProjectId<T>,
		},

		/// A Spend was created
		SpendCreated {
			when: ProvidedBlockNumberFor<T>,
			amount: BalanceOf<T>,
			project_id: ProjectId<T>,
		},

		/// Payment will be enacted for corresponding project
		WillBeEnacted { project_id: ProjectId<T> },

		/// Reward successfully assigned
		RewardsAssigned { when: ProvidedBlockNumberFor<T> },

		/// User's vote successfully submitted
		VoteCasted { who: VoterId<T>, when: ProvidedBlockNumberFor<T>, project_id: ProjectId<T> },

		/// User's vote successfully removed
		VoteRemoved { who: VoterId<T>, when: ProvidedBlockNumberFor<T>, project_id: ProjectId<T> },

		/// Project added to whitelisted projects list
		Projectlisted { when: ProvidedBlockNumberFor<T>, project_id: ProjectId<T> },

		/// Several projects added to whitelisted projects list
		Projectslisted { when: ProvidedBlockNumberFor<T>, projects_id: Vec<ProjectId<T>> },

		/// Project removed from whitelisted projects list
		ProjectUnlisted { when: ProvidedBlockNumberFor<T>, project_id: ProjectId<T> },

		/// Project Funding Accepted by voters
		ProjectFundingAccepted { project_id: ProjectId<T>, amount: BalanceOf<T> },

		/// Reward claim has expired
		ExpiredClaim { expired_when: ProvidedBlockNumberFor<T>, project_id: ProjectId<T> },

		/// Project Funding rejected by voters
		ProjectFundingRejected { project_id: ProjectId<T> },

		/// A new voting round started
		VotingRoundStarted { when: ProvidedBlockNumberFor<T>, round_number: u32 },

		/// The users voting period ended. Reward calculation will start.
		VoteActionLocked { when: ProvidedBlockNumberFor<T>, round_number: u32 },

		/// The voting round ended
		VotingRoundEnded { when: ProvidedBlockNumberFor<T>, round_number: u32 },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Not enough Funds in the Pot
		InsufficientPotReserves,
		/// The funds transfer operation failed
		TransferFailed,
		/// Spend or Spend index does not exists
		InexistentSpend,
		/// No valid Account_id found
		NoValidAccount,
		/// No project found under this project_id
		NoProjectAvailable,
		/// The Funds transfer failed
		FailedSpendOperation,
		/// Still not in claiming period
		NotClaimingPeriod,
		/// Still not in funds unlock period
		NotUnlockPeriod,
		/// An invalid result  was returned
		InvalidResult,
		/// The reward calculation failed due to an internal error
		FailedRewardCalculation,
		/// Voting round is over
		VotingRoundOver,
		/// This voting round does not exists
		NoRoundFound,
		/// Maximum number of projects submission for reward distribution as been reached
		MaximumProjectsNumber,
		/// Another project has already been submitted under the same project_id
		SubmittedProjectId,
		/// Project batch already submitted
		BatchAlreadySubmitted,
		/// Requested vote data do not exist
		NoVoteData,
		/// Not enough funds to process the transaction
		NotEnoughFunds,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<SystemBlockNumberFor<T>> for Pallet<T> {
		fn on_idle(_n: SystemBlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			Self::on_idle_function(remaining_weight)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// OPF Projects registration
		///
		/// ## Dispatch Origin
		///
		/// Must be AdminOrigin
		///
		/// ## Details
		///
		/// From this extrinsic only AdminOrigin can register whitelisted projects.
		///
		/// ### Parameters
		/// - `projects_id`: The accounts that might be funded.
		///
		/// ### Errors
		/// - [`Error::<T>::MaximumProjectsNumber`]: Maximum number of project subscriptions reached
		///  
		/// ## Events
		/// Emits [`Event::<T>::Projectslisted`].
		#[pallet::call_index(1)]
		#[pallet::weight(0)]
		pub fn register_projects_batch(
			origin: OriginFor<T>,
			projects_id: BoundedVec<ProjectId<T>, T::MaxProjects>,
		) -> DispatchResult {
			let who = T::SubmitOrigin::ensure_origin(origin.clone())?;
			// Only 1 batch submission per round
			let mut round_index = NextVotingRoundNumber::<T>::get();

			// No active round?
			if round_index == 0 {
				// Start the first voting round
				let _round0 = VotingRoundInfo::<T>::new();
				round_index = NextVotingRoundNumber::<T>::get();
			}

			let current_round_index = round_index.saturating_sub(1);

			let mut round_infos =
				VotingRounds::<T>::get(current_round_index).expect("InvalidResult");

			// Check no Project batch has been submitted yet
			ensure!(!round_infos.batch_submitted, Error::<T>::BatchAlreadySubmitted);
			round_infos.batch_submitted = true;
			let round_ending_block = round_infos.round_ending_block;

			// If current voting round is over, start a new one
			let when = T::BlockNumberProvider::current_block_number();
			if when >= round_ending_block {
				// Create a new round.
				let _new_round = VotingRoundInfo::<T>::new();
			}

			for project_id in &projects_id {
				ProjectInfo::<T>::new(project_id.clone());

				// Prepare the proposal call
				let call = Call::<T>::on_registration { project_id: project_id.clone() };
				let proposal = Self::create_proposal(who.clone(), call);

				// Start Referendum
				let referendum_index = Self::start_dem_referendum(
					proposal,
					<T as Democracy::Config>::EnactmentPeriod::get(),
				);
				let mut new_infos = WhiteListedProjectAccounts::<T>::get(&project_id)
					.ok_or(Error::<T>::NoProjectAvailable)?;
				new_infos.index = referendum_index;

				WhiteListedProjectAccounts::<T>::mutate(project_id, |value| {
					*value = Some(new_infos);
				});
			}
			VotingRounds::<T>::mutate(current_round_index, |round| *round = Some(round_infos));

			Self::deposit_event(Event::Projectslisted { when, projects_id: projects_id.to_vec() });
			Ok(())
		}

		/// OPF Projects de-listing
		///
		/// ## Dispatch Origin
		///
		/// Must be signed
		///
		/// ## Details
		///
		/// From this extrinsic only Root can de-list a project.
		///
		/// ### Parameters
		/// - `project_id`: The account that will receive the reward.
		///
		/// ### Errors
		/// - [`Error::<T>::NoProjectAvailable`]: No project found under this project_id
		///  
		/// ## Events
		/// Emits [`Event::<T>::ProjectUnlisted`].
		#[pallet::call_index(2)]
		#[pallet::weight(0)]
		pub fn unregister_project(
			origin: OriginFor<T>,
			project_id: ProjectId<T>,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin_or_root(origin)?;
			let when = T::BlockNumberProvider::current_block_number();
			WhiteListedProjectAccounts::<T>::remove(&project_id);
			Self::deposit_event(Event::<T>::ProjectUnlisted { when, project_id });

			Ok(())
		}

		/// OPF voting logic
		///
		/// ## Dispatch Origin
		///
		/// Must be signed
		///
		/// ## Details
		///
		/// This extrinsic allows users to [vote for/nominate] a whitelisted project using their
		/// funds. The amount defined by the user is locked and released only when the project
		/// reward is ready for distribution, or when the project is not dimmed fundable.
		/// Users can edit/over-write an existing vote within the vote-casting period.
		/// At the end of the voting period, rewards are calculated based on the total user amount
		/// attributed to each project by the user’s votes.
		///
		/// ### Parameters
		/// - `project_id`: The account that will receive the reward.
		/// - `amount`: Amount that will be locked in user’s balance to nominate a project.
		/// - `is_fund`: Parameter that defines if user’s vote is in favor (*true*), or against
		///   (*false*)
		/// the project funding.
		/// - `conviction`: Used to calculate the value allocated to the project, & determine
		/// when the voter's funds will be unlocked. Amount actually locked is the amount without
		/// conviction  
		///
		/// ### Errors
		/// - [`Error::<T>::NotEnoughFunds`]: The user does not have enough balance to cast a vote
		///  
		/// ## Events
		/// - [`Event::<T>::VoteCasted { who, when, project_id }`]: User's vote successfully
		///   submitted
		#[pallet::call_index(3)]
		#[pallet::weight(0)]
		pub fn vote(
			origin: OriginFor<T>,
			project_id: ProjectId<T>,
			#[pallet::compact] amount: BalanceOf<T>,
			is_fund: bool,
			conviction: Democracy::Conviction,
		) -> DispatchResult {
			let voter = ensure_signed(origin.clone())?;
			// Get current voting round & check if we are in voting period or not
			Self::period_check()?;
			// Check that voter has enough funds to vote
			let voter_balance = T::NativeBalance::total_balance(&voter);
			ensure!(voter_balance > amount, Error::<T>::NotEnoughFunds);

			// Check the available un-holded balance
			let voter_holds = T::NativeBalance::total_balance_on_hold(&voter);
			let available_funds = voter_balance.saturating_sub(voter_holds);
			ensure!(available_funds > amount, Error::<T>::NotEnoughFunds);

			let infos = WhiteListedProjectAccounts::<T>::get(&project_id)
				.ok_or(Error::<T>::NoProjectAvailable)?;
			let ref_index = infos.index;

			// Funds lock is handled by the opf pallet
			let conv = Democracy::Conviction::None;
			let vote = Democracy::Vote { aye: is_fund, conviction: conv };
			let converted_amount = Self::convert_balance(amount).ok_or("Failed Conversion!!!")?;
			let account_vote = Democracy::AccountVote::Standard { vote, balance: converted_amount };

			Self::try_vote(voter.clone(), project_id.clone(), amount, is_fund, conviction)?;
			Democracy::Pallet::<T>::vote(origin, ref_index, account_vote)?;

			let when = T::BlockNumberProvider::current_block_number();
			Self::deposit_event(Event::<T>::VoteCasted { who: voter, when, project_id });

			Ok(())
		}

		/// OPF vote removal logic
		///
		/// ## Dispatch Origin
		///
		/// Must be signed
		///
		/// ## Details
		///
		/// This extrinsic allows users to remove a casted vote, as long as it is within the
		/// vote-casting period.
		///
		/// ### Parameters
		/// - `project_id`: The account that will receive the reward.
		///
		/// ### Errors
		/// - [`Error::<T>::NoProjectAvailable`]: No project found under this project_id
		///  
		/// ## Events
		/// - [`Event::<T>::VoteRemoved { who, when, project_id }`]: User's vote successfully
		///   removed
		#[pallet::call_index(4)]
		#[pallet::weight(0)]
		pub fn remove_vote(origin: OriginFor<T>, project_id: ProjectId<T>) -> DispatchResult {
			let voter = ensure_signed(origin.clone())?;
			// Get current voting round & check if we are in voting period or not
			Self::period_check()?;
			// Removal action executed
			Self::try_remove_vote(voter.clone(), project_id.clone())?;
			// Remove previous vote from Referendum
			let infos = WhiteListedProjectAccounts::<T>::get(project_id.clone())
				.ok_or(Error::<T>::NoProjectAvailable)?;
			let ref_index = infos.index;
			Democracy::Pallet::<T>::remove_vote(origin, ref_index)?;

			let when = T::BlockNumberProvider::current_block_number();
			Self::deposit_event(Event::<T>::VoteRemoved { who: voter, when, project_id });
			Ok(())
		}

		/// OPF Reward Claim logic
		///
		/// ## Dispatch Origin
		///
		/// Must be signed
		///
		/// ## Details
		///
		/// From this extrinsic any user can claim a reward for a nominated/whitelisted project.
		///
		/// ### Parameters
		/// - `project_id`: The account that will receive the reward.
		///
		/// ### Errors
		/// - [`Error::<T>::InexistentSpend`]:Spend or Spend index does not exists
		/// - [`Error::<T>::NoValidAccount`]:  No valid Account_id found
		/// - [`Not Claiming Period`]: Still not in claiming period
		///  
		/// ## Events
		/// Emits [`Event::<T>::RewardClaimed`] if successful for a positive approval.
		#[pallet::call_index(5)]
		#[pallet::weight(0)]
		pub fn claim_reward_for(origin: OriginFor<T>, project_id: ProjectId<T>) -> DispatchResult {
			let _caller = ensure_signed(origin)?;
			let now = T::BlockNumberProvider::current_block_number();
			let info = Spends::<T>::get(&project_id).ok_or(Error::<T>::InexistentSpend)?;
			if now >= info.expire {
				Spends::<T>::remove(&project_id);
				Self::deposit_event(Event::ExpiredClaim { expired_when: info.expire, project_id });
				Ok(())
			} else if now < info.expire {
				// transfer the funds
				Self::spend(info.amount, project_id.clone())?;
				Self::deposit_event(Event::RewardClaimed {
					when: now,
					amount: info.amount,
					project_id: project_id.clone(),
				});
				WhiteListedProjectAccounts::<T>::remove(&project_id);
				Ok(())
			} else {
				// Claimin before proposal enactment
				Err(Error::<T>::NotClaimingPeriod.into())
			}
		}

		#[pallet::call_index(6)]
		#[pallet::weight(0)]
		pub fn on_registration(origin: OriginFor<T>, project_id: ProjectId<T>) -> DispatchResult {
			let _who = T::SubmitOrigin::ensure_origin(origin.clone())?;
			let infos = WhiteListedProjectAccounts::<T>::get(project_id.clone())
				.ok_or(Error::<T>::NoProjectAvailable)?;

			let ref_index = infos.index;
			let amount = infos.amount;
			let when = T::BlockNumberProvider::current_block_number();
			if let Some(ref_infos) = Democracy::ReferendumInfoOf::<T>::get(ref_index) {
				match ref_infos {
					Democracy::ReferendumInfo::Finished { approved: true, .. } => {
						let pot = Self::pot_account();
						let balance = T::NativeBalance::balance(&pot);
						let minimum_balance = T::NativeBalance::minimum_balance();
						// check if the pot has enough fund for the Spend
						// Check that the Pot as enough funds for the transfer
						let remaining_balance = balance.saturating_sub(infos.amount);
						ensure!(remaining_balance > minimum_balance, Error::<T>::NotEnoughFunds);
						// create a spend for project to be rewarded
						let new_spend = SpendInfo::<T>::new(&infos);
						Self::deposit_event(Event::ProjectFundingAccepted { project_id, amount });
						Self::deposit_event(Event::SpendCreated {
							when,
							amount: new_spend.amount,
							project_id: infos.project_id.clone(),
						});
					},
					Democracy::ReferendumInfo::Finished { approved: false, .. } =>
						Self::deposit_event(Event::ProjectFundingRejected { project_id }),
					Democracy::ReferendumInfo::Ongoing(_) => (),
				}
			}

			Ok(())
		}

		/// User's funds unlock
		///
		/// ## Dispatch Origin
		///
		/// Must be signed
		///
		/// ## Details
		///
		/// This extrinsic allows users to unlock funds related to a specific project,
		/// provided the locking period (which is dependant of the conviction) has ended.
		///
		/// ### Parameters
		/// - `project_id`: The account that will receive the reward.
		///
		/// ### Errors
		/// - [`Error::<T>::NoVoteData`]: No vote data found for the specified project
		///  
		/// ## Events
		#[pallet::call_index(7)]
		#[pallet::weight(0)]
		pub fn release_voter_funds(
			origin: OriginFor<T>,
			project_id: ProjectId<T>,
		) -> DispatchResult {
			let voter_id = ensure_signed(origin)?;
			ensure!(Votes::<T>::contains_key(&project_id, &voter_id), Error::<T>::NoVoteData);
			let infos = Votes::<T>::get(&project_id, &voter_id).ok_or(Error::<T>::NoVoteData)?;
			let release_block = infos.funds_unlock_block;
			let amount = infos.amount;

			let now = T::BlockNumberProvider::current_block_number();
			ensure!(now >= release_block, Error::<T>::NotUnlockPeriod);
			T::NativeBalance::release(
				&HoldReason::FundsReserved.into(),
				&voter_id,
				amount,
				Precision::Exact,
			)?;

			Votes::<T>::remove(&project_id, &voter_id);
			Ok(())
		}

		#[pallet::call_index(8)]
		#[pallet::weight(0)]
		pub fn execute_call_dispatch(
			origin: OriginFor<T>,
			caller: T::AccountId,
			proposal: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResult {
			ensure_root(origin)?;
			proposal
				.dispatch_bypass_filter(frame_system::RawOrigin::Signed(caller.clone()).into())
				.ok();
			Ok(().into())
		}
	}
}
