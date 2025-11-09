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

//! # Loans Pallet
//!
//! A pallet for managing Collateralized Debt Positions (CDPs)
//!
//! ## Pallet API
//!
//! See the [`pallet`] module for more information about the interfaces this pallet exposes,
//! including its configuration trait, dispatchables, storage items, events and errors.
//!
//! ## Overview
//!
//! The Loans pallet is responsible for locking collateral
//! assets and borrowing stable currency against them.
//!
//! Key features include:
//!
//! - **Position Management**: Responsible for managing CDP positions, including adjusting
//!   collateral and debit balances, transferring loan positions between accounts, and executing
//!   liquidations.
//! - **CDP Treasury Integration**: issues/burns debits, handles bad debts via the `T::CDPTreasury`,
//! - **Risk Management Integration**: position risk and total debit checks via the `T::RiskManager`
//! - **Liquidation Strategy Integration**: liquidating CDP positions via the
//!   `T::LiquidationStrategy`
//!
//! ### Terminology
//!
//! - **CDP (Collateralized Debt Position)**: A position where a user locks collateral and borrows
//!   stable currency
//! - **Collateral**: Assets locked by users to back their borrowed stable currency
//! - **Debit**: The amount of stable currency borrowed against collateral
//! - **Stability Fee**: Interest rate applied to borrowed amounts
//! - **Liquidation**: Process of selling collateral to repay debt when position becomes risky
#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible::{hold::Mutate as HoldMutate, Inspect},
		honzon::{
			CDPTreasury, Debit, DebitUnit, Handler, LiquidationTarget, Position, Rate, RiskManager,
		},
		tokens::{Fortitude, Precision, Restriction},
	},
	transactional, PalletId,
};
pub use pallet::*;
use sp_runtime::{
	traits::{AccountIdConversion, Saturating, Zero},
	DispatchResult,
};
use sp_std::prelude::*;

pub type BalanceOf<T> =
	<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;
type DebitUnitOf<T> = DebitUnit<BalanceOf<T>>;
type PositionOf<T> = Position<DebitUnitOf<T>, BalanceOf<T>, Rate>;

mod adjustment;
pub use adjustment::BalanceAdjustment;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Currency type for handling collateral holds on user accounts.
		type Currency: HoldMutate<Self::AccountId, Reason = Self::RuntimeHoldReason>;

		/// Runtime hold reason type that can represent this pallet's reasons.
		type RuntimeHoldReason: From<HoldReason>;

		/// Risk manager for limiting the total debit size and checking position validity
		type RiskManager: RiskManager<DebitUnitOf<Self>, BalanceOf<Self>, Rate>;

		/// CDP treasury for issuing/burning stable currency and handling bad debts
		type CDPTreasury: CDPTreasury<Self::AccountId, Balance = BalanceOf<Self>>;

		/// The loan\'s module id, keep all collaterals of CDPs.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// Event handler which calls when update loan.
		type OnUpdateLoan: Handler<(
			Self::AccountId,
			BalanceAdjustment<BalanceOf<Self>>,
			BalanceOf<Self>,
		)>;

		/// Liquidation strategy for liquidating CDP positions
		type LiquidationStrategy: LiquidationTarget<Self::AccountId, BalanceOf<Self>>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Position updated.
		PositionUpdated {
			owner: T::AccountId,
			collateral_adjustment: BalanceAdjustment<BalanceOf<T>>,
			debit_value_adjustment: BalanceAdjustment<BalanceOf<T>>,
		},
		/// Confiscate CDP\'s collateral assets and eliminate its debit.
		ConfiscateCollateralAndDebit {
			owner: T::AccountId,
			confiscated_collateral_amount: BalanceOf<T>,
			deduct_debit_value: BalanceOf<T>,
		},
		/// Transfer loan.
		TransferLoan { from: T::AccountId, to: T::AccountId },
	}

	/// The collateralized debit positions.
	///
	/// Positions: AccountId => Position
	#[pallet::storage]
	#[pallet::getter(fn positions)]
	pub type Positions<T: Config> =
		StorageMap<_, Twox64Concat, T::AccountId, PositionOf<T>, ValueQuery>;

	/// Total debit indexed by the stability fee applied to the position.
	#[pallet::storage]
	#[pallet::getter(fn total_debit_by_stability_fee)]
	pub type TotalDebitByStabilityFee<T: Config> =
		StorageMap<_, Twox64Concat, Rate, DebitUnitOf<T>, ValueQuery>;

	/// Reasons for placing holds on user collateral.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Collateral locked to back a loan position.
		#[codec(index = 0)]
		Collateral,
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	impl<T: Config> Pallet<T> {
		pub fn account_id() -> T::AccountId {
			T::PalletId::get().into_account_truncating()
		}

		/// confiscate collateral and debit to cdp treasury.
		///
		/// Ensured atomic.
		#[transactional]
		pub fn confiscate_collateral_and_debit(
			who: &T::AccountId,
			collateral_confiscate: BalanceOf<T>,
			debit_value_to_cover: BalanceOf<T>,
		) -> DispatchResult {
			let module_account = Self::account_id();
			// Move the collateral from the user's held balance into the pallet account before
			// liquidating or forwarding the remainder to the treasury.
			let _ = T::Currency::transfer_on_hold(
				&HoldReason::Collateral.into(),
				who,
				&module_account,
				collateral_confiscate,
				Precision::Exact,
				Restriction::Free,
				Fortitude::Force,
			)?;

			let (liquidated_collateral, covered_debit_value) = T::LiquidationStrategy::liquidate(
				&module_account,
				collateral_confiscate,
				debit_value_to_cover,
			)?;

			let remaining_collateral = collateral_confiscate.saturating_sub(liquidated_collateral);
			let remaining_debit_value = debit_value_to_cover.saturating_sub(covered_debit_value);

			// CDP Treasury handles the remaining position to be confiscate
			if !remaining_collateral.is_zero() {
				// transfer remaining collateral to cdp treasury
				T::CDPTreasury::deposit_collateral(&module_account, remaining_collateral)?;
			}

			// deposit remaining debit to cdp treasury
			if !remaining_debit_value.is_zero() {
				T::CDPTreasury::on_system_debit(remaining_debit_value)?;
			}

			// update loan - decrease both collateral and debit
			let collateral_adjustment = BalanceAdjustment::decrease(collateral_confiscate);
			let debit_value_adjustment = BalanceAdjustment::decrease(debit_value_to_cover);
			Self::update_loan(who, collateral_adjustment, debit_value_adjustment)?;

			Self::deposit_event(Event::ConfiscateCollateralAndDebit {
				owner: who.clone(),
				confiscated_collateral_amount: collateral_confiscate,
				deduct_debit_value: debit_value_to_cover,
			});
			Ok(())
		}

		/// transfer whole loan of `from` to `to`
		///
		/// Ensured atomic.
		#[transactional]
		pub fn transfer_loan(from: &T::AccountId, to: &T::AccountId) -> DispatchResult {
			let Position { collateral: from_collateral, debit: from_debit } = Self::positions(from);
			let from_debit_value =
				T::RiskManager::debit_units_to_value(from_debit.stability_fee, from_debit.units);

			Self::update_loan(
				from,
				BalanceAdjustment::Decrease(from_collateral),
				BalanceAdjustment::Decrease(from_debit_value),
			)?;
			Self::update_loan(
				to,
				BalanceAdjustment::Increase(from_collateral),
				BalanceAdjustment::Increase(from_debit_value),
			)?;

			// TODO: Check if the accessing can be optimized
			let Position { collateral, debit } = Self::positions(to);

			// check new position
			T::RiskManager::check_position_valid(
				collateral,
				debit.units,
				debit.stability_fee,
				true,
			)?;

			Self::deposit_event(Event::TransferLoan { from: from.clone(), to: to.clone() });
			Ok(())
		}

		/// mutate accounting records of collaterals and debits
		///
		/// Currently, we use previous stability fee in account
		pub(crate) fn update_loan(
			who: &T::AccountId,
			collateral_adjustment: BalanceAdjustment<BalanceOf<T>>,
			debit_value_adjustment: BalanceAdjustment<BalanceOf<T>>,
		) -> DispatchResult {
			<Positions<T>>::try_mutate_exists(who, |may_be_position| -> DispatchResult {
				let mut p = may_be_position.take().unwrap_or_else(|| {
					if frame_system::Pallet::<T>::inc_consumers(who).is_err() {
						// No providers for the locks. This is impossible under normal circumstances
						// since the funds that are under the lock will themselves be stored in the
						// account and therefore will need a reference.
						log::warn!(
						"Warning: Attempt to introduce lock consumer reference, yet no providers. \
							This is unexpected but should be safe."
					);
					}
					Default::default()
				});

				// Calculate new collateral
				let new_collateral = collateral_adjustment.apply_to(p.collateral)?;

				let previous_debit_value =
					T::RiskManager::debit_units_to_value(p.debit.stability_fee, p.debit.units);

				// Calculate new debit value
				let new_debit_value = debit_value_adjustment.apply_to(previous_debit_value)?;

				// update debit units use previous stability fee
				let new_debit_units =
					T::RiskManager::value_to_debit_units(p.debit.stability_fee, new_debit_value);
				let new_debit =
					{ Debit { units: new_debit_units, stability_fee: p.debit.stability_fee } };

				// use the collateral amount as the shares for Loans incentives
				T::OnUpdateLoan::handle(&(who.clone(), collateral_adjustment, p.collateral))?;

				let debit_units_adjustment = if debit_value_adjustment.is_increase() {
					BalanceAdjustment::increase(new_debit_units.saturating_sub(p.debit.units))
				} else {
					BalanceAdjustment::decrease(p.debit.units.saturating_sub(new_debit_units))
				};

				// Update total debit by stability fee
				if !debit_units_adjustment.is_zero() {
					TotalDebitByStabilityFee::<T>::try_mutate(
						p.debit.stability_fee,
						|total| -> DispatchResult {
							*total = debit_units_adjustment.apply_to(*total)?;
							Ok(())
						},
					)?;
				}

				// Update position
				p.collateral = new_collateral;
				p.debit = new_debit;

				// decrease account ref if zero position
				if p.collateral.is_zero() && p.debit.units.is_zero() {
					frame_system::Pallet::<T>::dec_consumers(who);
					// remove position storage if zero position
					*may_be_position = None;
				} else {
					*may_be_position = Some(p);
				}

				Ok(())
			})?;

			Self::deposit_event(Event::PositionUpdated {
				owner: who.clone(),
				collateral_adjustment,
				debit_value_adjustment,
			});
			Ok(())
		}
	}
}
