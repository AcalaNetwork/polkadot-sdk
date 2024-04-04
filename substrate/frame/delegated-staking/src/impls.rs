// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Implementations of public traits, namely [DelegationInterface] and [OnStakingUpdate].

use super::*;
use sp_staking::{DelegationInterface, OnStakingUpdate};

impl<T: Config> DelegationInterface for Pallet<T> {
	type Balance = BalanceOf<T>;
	type AccountId = T::AccountId;

	/// Effective balance of the `Agent` account.
	fn agent_balance(who: &Self::AccountId) -> Self::Balance {
		Agent::<T>::from(who)
			.map(|agent| agent.ledger.effective_balance())
			.unwrap_or_default()
	}

	fn delegator_balance(delegator: &Self::AccountId) -> Self::Balance {
		Delegation::<T>::get(delegator).map(|d| d.amount).unwrap_or_default()
	}

	/// Delegate funds to an `Agent`.
	fn delegate(
		who: &Self::AccountId,
		agent: &Self::AccountId,
		reward_account: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		Pallet::<T>::register_agent(
			RawOrigin::Signed(agent.clone()).into(),
			reward_account.clone(),
		)?;

		// Delegate the funds from who to the `Agent` account.
		Pallet::<T>::delegate_to_agent(RawOrigin::Signed(who.clone()).into(), agent.clone(), amount)
	}

	/// Add more delegation to the `Agent` account.
	fn delegate_extra(
		who: &Self::AccountId,
		agent: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		Pallet::<T>::delegate_to_agent(RawOrigin::Signed(who.clone()).into(), agent.clone(), amount)
	}

	/// Withdraw delegation of `delegator` to `Agent`.
	///
	/// If there are funds in `Agent` account that can be withdrawn, then those funds would be
	/// unlocked/released in the delegator's account.
	fn withdraw_delegation(
		delegator: &Self::AccountId,
		agent: &Self::AccountId,
		amount: Self::Balance,
	) -> DispatchResult {
		// fixme(ank4n): Can this not require slashing spans?
		Pallet::<T>::release_delegation(
			RawOrigin::Signed(agent.clone()).into(),
			delegator.clone(),
			amount,
			0,
		)
	}

	/// Returns true if the `Agent` have any slash pending to be applied.
	fn has_pending_slash(agent: &Self::AccountId) -> bool {
		Agent::<T>::from(agent)
			.map(|d| !d.ledger.pending_slash.is_zero())
			.unwrap_or(false)
	}

	fn delegator_slash(
		agent: &Self::AccountId,
		delegator: &Self::AccountId,
		value: Self::Balance,
		maybe_reporter: Option<Self::AccountId>,
	) -> sp_runtime::DispatchResult {
		Pallet::<T>::do_slash(agent.clone(), delegator.clone(), value, maybe_reporter)
	}

	fn migrate_nominator_to_agent(
		agent: &Self::AccountId,
		reward_account: &Self::AccountId,
	) -> DispatchResult {
		Pallet::<T>::migrate_to_agent(
			RawOrigin::Signed(agent.clone()).into(),
			reward_account.clone(),
		)
	}

	fn migrate_delegation(
		agent: &Self::AccountId,
		delegator: &Self::AccountId,
		value: Self::Balance,
	) -> DispatchResult {
		Pallet::<T>::claim_delegation(
			RawOrigin::Signed(agent.clone()).into(),
			delegator.clone(),
			value,
		)
	}
}

impl<T: Config> OnStakingUpdate<T::AccountId, BalanceOf<T>> for Pallet<T> {
	fn on_slash(
		who: &T::AccountId,
		_slashed_active: BalanceOf<T>,
		_slashed_unlocking: &sp_std::collections::btree_map::BTreeMap<EraIndex, BalanceOf<T>>,
		slashed_total: BalanceOf<T>,
	) {
		<Agents<T>>::mutate(who, |maybe_register| match maybe_register {
			// if existing agent, register the slashed amount as pending slash.
			Some(register) => register.pending_slash.saturating_accrue(slashed_total),
			None => {
				// nothing to do
			},
		});
	}

	fn on_withdraw(stash: &T::AccountId, amount: BalanceOf<T>) {
		// if there is a withdraw to the agent, then add it to the unclaimed withdrawals.
		if let Ok(agent) = Agent::<T>::from(stash) {
			let agent = agent.add_unclaimed_withdraw(amount).defensive();

			// can't do anything if there is an overflow error.
			if agent.is_ok() {
				agent.expect("checked above; qed").save();
			}
		}
	}
}
