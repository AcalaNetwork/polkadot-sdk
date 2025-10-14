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

pub use pallet::*;

mod adjustment;
pub use adjustment::BalanceAdjustment;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use crate::BalanceAdjustment;
	use frame_support::{
		pallet_prelude::*,
		traits::{
			fungible::{hold::Mutate as HoldMutate, Inspect},
			honzon::{CDPTreasury, Handler, LiquidationTarget, Position, Rate, Ratio, RiskManager},
			tokens::{Fortitude, Precision, Restriction},
		},
		transactional, PalletId,
	};
	use sp_runtime::{
		traits::{AccountIdConversion, Saturating, Zero},
		DispatchResult,
	};
	use sp_std::prelude::*;

	pub type BalanceOf<T> =
		<<T as Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Currency type for handling collateral holds on user accounts.
		type Currency: HoldMutate<Self::AccountId, Reason = Self::RuntimeHoldReason>;

		/// Runtime hold reason type that can represent this pallet's reasons.
		type RuntimeHoldReason: From<HoldReason>;

		/// The currency ID type
		type CurrencyId: Parameter + Member + Copy + MaybeSerializeDeserialize + Ord;

		/// Risk manager for limiting the total debit size and checking position validity
		type RiskManager: RiskManager<
			Self::AccountId,
			Self::CurrencyId,
			BalanceOf<Self>,
			BalanceOf<Self>,
		>;

		/// CDP treasury for issuing/burning stable currency and handling bad debts
		type CDPTreasury: CDPTreasury<
			Self::AccountId,
			CurrencyId = Self::CurrencyId,
			Balance = BalanceOf<Self>,
		>;

		/// The loan\'s module id, keep all collaterals of CDPs.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// The asset ID of the collateral currency.
		#[pallet::constant]
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
			debit_adjustment: BalanceAdjustment<BalanceOf<T>>,
		},
		/// Confiscate CDP\'s collateral assets and eliminate its debit.
		ConfiscateCollateralAndDebit {
			owner: T::AccountId,
			confiscated_collateral_amount: BalanceOf<T>,
			deduct_debit_amount: BalanceOf<T>,
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
		StorageMap<_, Twox64Concat, T::AccountId, Position<BalanceOf<T>>, ValueQuery>;

	/// The total collateralized debit positions.
	///
	/// TotalPositions: () => Position
	#[pallet::storage]
	#[pallet::getter(fn total_positions)]
	pub type TotalPositions<T: Config> = StorageValue<_, Position<BalanceOf<T>>, ValueQuery>;

	/// Total debit indexed by the stability fee applied to the position.
	#[pallet::storage]
	#[pallet::getter(fn total_debit_by_stability_fee)]
	pub type TotalDebitByStabilityFee<T: Config> =
		StorageMap<_, Twox64Concat, Rate, BalanceOf<T>, ValueQuery>;

	/// Reasons for placing holds on user collateral.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Collateral locked to back a loan position.
		#[codec(index = 0)]
		Collateral,
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::call]
	impl<T: Config> Pallet<T> {}

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
			debit_decrease: BalanceOf<T>,
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

			let (liquidated_collateral, covered_debit) = T::LiquidationStrategy::liquidate(
				&module_account,
				T::CollateralCurrencyId::get(),
				collateral_confiscate,
				debit_decrease,
			)?;

			let remaining_collateral = collateral_confiscate.saturating_sub(liquidated_collateral);
			let remaining_debit = debit_decrease.saturating_sub(covered_debit);

			// CDP Treasury handles the remaining position to be confiscate
			if !remaining_collateral.is_zero() {
				// transfer remaining collateral to cdp treasury
				T::CDPTreasury::deposit_collateral(&module_account, remaining_collateral)?;
			}

			if !remaining_debit.is_zero() {
				// deposit remaining debit to cdp treasury
				let bad_debt_value = T::RiskManager::get_debit_value(
					T::CollateralCurrencyId::get(),
					remaining_debit,
				);
				T::CDPTreasury::on_system_debit(bad_debt_value)?;
			}

			// update loan - decrease both collateral and debit
			let collateral_adjustment = BalanceAdjustment::decrease(collateral_confiscate);
			let debit_adjustment = BalanceAdjustment::decrease(debit_decrease);
			Self::update_loan(who, collateral_adjustment, debit_adjustment, None)?;

			Self::deposit_event(Event::ConfiscateCollateralAndDebit {
				owner: who.clone(),
				confiscated_collateral_amount: collateral_confiscate,
				deduct_debit_amount: debit_decrease,
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
			debit_adjustment: BalanceAdjustment<BalanceOf<T>>,
			maybe_new_stability_fee: Option<Rate>,
		) -> DispatchResult {
			// mutate collateral and debit accounting records
			// Note: if a new position, will inc consumer
			Self::update_loan(
				who,
				collateral_adjustment,
				debit_adjustment,
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
			match &debit_adjustment {
				BalanceAdjustment::Increase(amount) => {
					// check debit cap when increase debit
					T::RiskManager::check_debit_cap(
						T::CollateralCurrencyId::get(),
						Self::total_positions().debit,
					)?;

					// issue debit with collateral backed by cdp treasury
					T::CDPTreasury::issue_debit(
						who,
						T::RiskManager::get_debit_value(T::CollateralCurrencyId::get(), *amount),
						true,
					)?;
				},
				BalanceAdjustment::Decrease(amount) => {
					// repay debit
					// burn debit by cdp treasury
					T::CDPTreasury::burn_debit(
						who,
						T::RiskManager::get_debit_value(T::CollateralCurrencyId::get(), *amount),
					)?;
				},
			}

			// ensure pass risk check
			let Position { collateral, debit, .. } = Self::positions(who);
			let is_risk_increasing =
				collateral_adjustment.is_decrease() || debit_adjustment.is_increase();
			T::RiskManager::check_position_valid(
				T::CollateralCurrencyId::get(),
				collateral,
				debit,
				is_risk_increasing,
			)?;

			Ok(())
		}

		/// transfer whole loan of `from` to `to`
		///
		/// Ensured atomic.
		#[transactional]
		pub fn transfer_loan(from: &T::AccountId, to: &T::AccountId) -> DispatchResult {
			// get `from` position data
			let Position { collateral, debit, stability_fee } = Self::positions(from);
			let from_stability_fee = Rate::from_inner(stability_fee.into_inner());

			let Position { collateral: to_collateral, debit: to_debit, .. } = Self::positions(to);
			let new_to_collateral_balance = to_collateral
				.checked_add(&collateral)
				.expect("existing collateral balance cannot overflow; qed");
			let new_to_debit_balance = to_debit
				.checked_add(&debit)
				.expect("existing debit balance cannot overflow; qed");

			// check new position
			T::RiskManager::check_position_valid(
				T::CollateralCurrencyId::get(),
				new_to_collateral_balance,
				new_to_debit_balance,
				true,
			)?;

			Self::update_loan(
				from,
				BalanceAdjustment::Decrease(collateral),
				BalanceAdjustment::Decrease(debit),
				None,
			)?;
			Self::update_loan(
				to,
				BalanceAdjustment::Increase(collateral),
				BalanceAdjustment::Increase(debit),
				Some(from_stability_fee),
			)?;

			Self::deposit_event(Event::TransferLoan { from: from.clone(), to: to.clone() });
			Ok(())
		}

		/// mutate accounting records of collaterals and debits
		pub(crate) fn update_loan(
			who: &T::AccountId,
			collateral_adjustment: BalanceAdjustment<BalanceOf<T>>,
			debit_adjustment: BalanceAdjustment<BalanceOf<T>>,
			maybe_new_stability_fee: Option<Rate>,
		) -> DispatchResult {
			<Positions<T>>::try_mutate_exists(who, |may_be_position| -> DispatchResult {
				let mut p = may_be_position.take().unwrap_or_default();
				let previous_debit = p.debit;
				let previous_stability_fee = p.stability_fee;
				let new_collateral = collateral_adjustment.apply_to(p.collateral)?;
				let new_debit = debit_adjustment.apply_to(p.debit)?;
				let mut next_stability_fee =
					if new_debit.is_zero() { Ratio::zero() } else { p.stability_fee };
				if let Some(stability_fee) = maybe_new_stability_fee {
					if debit_adjustment.is_increase() && !new_debit.is_zero() {
						next_stability_fee = Ratio::from_inner(stability_fee.into_inner());
					}
				}

				// increase account ref if new position
				if p.collateral.is_zero() && p.debit.is_zero() {
					if frame_system::Pallet::<T>::inc_consumers(who).is_err() {
						// No providers for the locks. This is impossible under normal circumstances
						// since the funds that are under the lock will themselves be stored in the
						// account and therefore will need a reference.
						log::warn!(
							"Warning: Attempt to introduce lock consumer reference, yet no providers. \
							This is unexpected but should be safe."
						);
					}
				}

				// use the collateral amount as the shares for Loans incentives
				T::OnUpdateLoan::handle(&(who.clone(), collateral_adjustment, p.collateral))?;
				p.collateral = new_collateral;
				p.debit = new_debit;
				p.stability_fee = next_stability_fee;

				// Remove the previous debit stability fee tracking record
				if !previous_debit.is_zero() {
					let previous_fee_key = Rate::from_inner(previous_stability_fee.into_inner());
					TotalDebitByStabilityFee::<T>::mutate_exists(previous_fee_key, |maybe_total| {
						if let Some(total) = maybe_total {
							*total = total.saturating_sub(previous_debit);
							if total.is_zero() {
								*maybe_total = None;
							}
						}
					});
				}

				// Add the new debit stability in fee tracking
				if !p.debit.is_zero() {
					let new_fee_key = Rate::from_inner(p.stability_fee.into_inner());
					TotalDebitByStabilityFee::<T>::mutate(new_fee_key, |total| {
						*total = total.saturating_add(p.debit);
					});
				}

				// decrease account ref if zero position
				if p.collateral.is_zero() && p.debit.is_zero() {
					frame_system::Pallet::<T>::dec_consumers(who);

					// remove position storage if zero position
					*may_be_position = None;
				} else {
					*may_be_position = Some(p);
				}

				Ok(())
			})?;

			// update total positions
			TotalPositions::<T>::try_mutate(|total_positions| -> DispatchResult {
				total_positions.collateral =
					collateral_adjustment.apply_to(total_positions.collateral)?;
				total_positions.debit = debit_adjustment.apply_to(total_positions.debit)?;
				Ok(())
			})?;

			Self::deposit_event(Event::PositionUpdated {
				owner: who.clone(),
				collateral_adjustment,
				debit_adjustment,
			});
			Ok(())
		}
	}
}
