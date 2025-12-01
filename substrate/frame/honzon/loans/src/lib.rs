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
pub type DebitUnitOf<T> = DebitUnit<BalanceOf<T>>;
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
		/// The currency ID type.
		type CurrencyId: Parameter + Member + Copy + MaybeSerializeDeserialize + Ord + MaxEncodedLen;

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

		/// The currency ID of the collateral.
		type CollateralCurrencyId: Get<Self::CurrencyId>;

		/// Event handler which calls when update loan.
		type OnUpdateLoan: Handler<(
			Self::AccountId,
			BalanceAdjustment<BalanceOf<Self>>,
			BalanceOf<Self>,
		)>;

		/// Liquidation strategy for liquidating CDP positions
		type LiquidationStrategy: LiquidationTarget<
			Self::AccountId,
			Self::CurrencyId,
			BalanceOf<Self>,
		>;
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
		/// First, the collateral will be transferred to the CDP treasury.
		/// Then, T::LiquidationStrategy will try to liquidate the collateral and debit.
		/// Finally, the remaining bad debt will be deposited into the CDP treasury.
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

			// liquidate on behalf of the CDP treasury.
			let (liquidated_collateral, covered_debit_value) = T::LiquidationStrategy::liquidate(
				&module_account,
				T::CollateralCurrencyId::get(),
				collateral_confiscate,
				debit_value_to_cover,
			)?;

			let remaining_collateral = collateral_confiscate.saturating_sub(liquidated_collateral);
			let remaining_debit_value = debit_value_to_cover.saturating_sub(covered_debit_value);

			// deposit remaining collateral to cdp treasury.
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
			Self::update_loan(who, collateral_adjustment, debit_value_adjustment, None)?;

			Self::deposit_event(Event::ConfiscateCollateralAndDebit {
				owner: who.clone(),
				confiscated_collateral_amount: collateral_confiscate,
				deduct_debit_value: debit_value_to_cover,
			});
			Ok(())
		}
		/// adjust the position.
		///
		/// Ensured atomic.
		#[transactional]
		pub fn adjust_position(
			who: &T::AccountId,
			collateral_adjustment: BalanceAdjustment<BalanceOf<T>>,
			debit_value_adjustment: BalanceAdjustment<BalanceOf<T>>,
			maybe_new_stability_fee: Option<Rate>,
		) -> DispatchResult {
			// mutate collateral and debit accounting records
			// Note: if a new position, will inc consumer
			Self::update_loan(
				who,
				collateral_adjustment,
				debit_value_adjustment,
				maybe_new_stability_fee,
			)?;

			// Handle collateral adjustments
			match &collateral_adjustment {
				BalanceAdjustment::Increase(amount) => {
					T::Currency::hold(&HoldReason::Collateral.into(), who, *amount)?;
				},
				BalanceAdjustment::Decrease(amount) => {
					let _ = T::Currency::release(
						&HoldReason::Collateral.into(),
						who,
						*amount,
						Precision::Exact,
					)?;
				},
			}

			// Handle debit adjustments
			match &debit_value_adjustment {
				BalanceAdjustment::Increase(amount) => {
					// check debit cap when increase debit
					T::RiskManager::check_debit_cap()?;

					// issue debit with collateral backed by cdp treasury
					T::CDPTreasury::issue_debit(who, *amount, true)?;
				},
				BalanceAdjustment::Decrease(amount) => {
					// repay debit
					// burn debit by cdp treasury
					T::CDPTreasury::burn_debit(who, *amount)?;
				},
			}

			// ensure pass risk check
			let Position { collateral, debit, .. } = Self::positions(who);

			T::RiskManager::check_position_valid(
				collateral,
				debit.units,
				debit.stability_fee,
				true,
			)?;

			Ok(())
		}
		/// transfer whole loan of `from` to `to`
		///
		/// Ensured atomic.
		#[transactional]
		pub fn transfer_loan(from: &T::AccountId, to: &T::AccountId) -> DispatchResult {
			let Position { collateral: from_collateral, debit: from_debit } = Self::positions(from);
			let from_debit_value =
				T::RiskManager::debit_units_to_value(from_debit.units, from_debit.stability_fee);

			Self::update_loan(
				from,
				BalanceAdjustment::Decrease(from_collateral),
				BalanceAdjustment::Decrease(from_debit_value),
				None,
			)?;
			Self::update_loan(
				to,
				BalanceAdjustment::Increase(from_collateral),
				BalanceAdjustment::Increase(from_debit_value),
				None,
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
		/// If maybe_new_stability_fee is provided, use it as the new stability fee.
		/// otherwise, use previous stability fee.
		pub(crate) fn update_loan(
			who: &T::AccountId,
			collateral_adjustment: BalanceAdjustment<BalanceOf<T>>,
			debit_value_adjustment: BalanceAdjustment<BalanceOf<T>>,
			maybe_new_stability_fee: Option<Rate>,
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
					T::RiskManager::debit_units_to_value(p.debit.units, p.debit.stability_fee);

				// Calculate new debit value
				let new_debit_value = debit_value_adjustment.apply_to(previous_debit_value)?;

				// update debit units use new stability fee if provided, otherwise use previous
				// stability fee
				let new_debit = match maybe_new_stability_fee {
					Some(new_stability_fee) => {
						let new_debit_units = T::RiskManager::value_to_debit_units(
							new_debit_value,
							new_stability_fee,
						);
						// Update total debit by previous stability fee
						TotalDebitByStabilityFee::<T>::try_mutate(
							p.debit.stability_fee,
							|total| -> DispatchResult {
								let debit_units_adjustment =
									BalanceAdjustment::decrease(p.debit.units);
								*total = debit_units_adjustment.apply_to(*total)?;
								Ok(())
							},
						)?;
						// Update total debit by new stability fee
						TotalDebitByStabilityFee::<T>::try_mutate(
							new_stability_fee,
							|total| -> DispatchResult {
								let debit_units_adjustment =
									BalanceAdjustment::increase(new_debit_units);
								*total = debit_units_adjustment.apply_to(*total)?;
								Ok(())
							},
						)?;
						Debit { units: new_debit_units, stability_fee: new_stability_fee }
					},
					None => {
						let new_debit_units = T::RiskManager::value_to_debit_units(
							new_debit_value,
							p.debit.stability_fee,
						);
						let debit_units_adjustment = if debit_value_adjustment.is_increase() {
							BalanceAdjustment::increase(
								new_debit_units.saturating_sub(p.debit.units),
							)
						} else {
							BalanceAdjustment::decrease(
								p.debit.units.saturating_sub(new_debit_units),
							)
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
						Debit { units: new_debit_units, stability_fee: p.debit.stability_fee }
					},
				};

				// use the collateral amount as the shares for Loans incentives
				T::OnUpdateLoan::handle(&(who.clone(), collateral_adjustment, p.collateral))?;

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
