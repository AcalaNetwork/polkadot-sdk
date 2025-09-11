// This file is part of Acala.

// Copyright (C) 2020-2025 Acala Foundation.
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

//! # Loans Module
//!
//! ## Overview
//!
//! Loans module manages CDP\'s collateral assets and the debits backed by these
//! assets.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::unused_unit)]
#![allow(clippy::collapsible_if)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{
		pallet_prelude::*,
		traits::{Currency, ExistenceRequirement, ReservableCurrency},
		transactional, PalletId,
	};
	use pallet_traits::{CDPTreasury, Handler, Position, RiskManager};
	use sp_runtime::{
		traits::{AccountIdConversion, Zero},
		ArithmeticError, DispatchResult,
	};
	use sp_std::prelude::*;

	pub type Amount = i128;
	pub type BalanceOf<T> = <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Currency type for deposit/withdraw collateral assets to/from loans
		/// module
		type Currency: ReservableCurrency<Self::AccountId>;

		/// The currency ID type
		type CurrencyId: Parameter + Member + Copy + MaybeSerializeDeserialize + Ord;

		/// Risk manager is used to limit the debit size of CDP
		type RiskManager: RiskManager<Self::AccountId, Self::CurrencyId, BalanceOf<Self>, BalanceOf<Self>>;

		/// CDP treasury for issuing/burning stable currency adjust debit value
		/// adjustment
		type CDPTreasury: CDPTreasury<Self::AccountId, CurrencyId = Self::CurrencyId, Balance = BalanceOf<Self>>;

		/// The loan\'s module id, keep all collaterals of CDPs.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// The asset ID of the collateral currency.
		#[pallet::constant]
		type CollateralCurrencyId: Get<Self::CurrencyId>;

		/// Event handler which calls when update loan.
		type OnUpdateLoan: Handler<(Self::AccountId, Amount, BalanceOf<Self>)>;
	}

	#[pallet::error]
	pub enum Error<T> {
		AmountConvertFailed,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Position updated.
		PositionUpdated {
			owner: T::AccountId,
			collateral_adjustment: Amount,
			debit_adjustment: Amount,
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

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::call]
	impl<T: Config> Pallet<T> {}

	impl<T: Config> Pallet<T>
	where
		BalanceOf<T>: TryFrom<Amount> + TryInto<Amount>,
	{
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
			// convert balance type to amount type
			let collateral_adjustment = Self::amount_try_from_balance(collateral_confiscate)?;
			let debit_adjustment = Self::amount_try_from_balance(debit_decrease)?;

			// transfer collateral to cdp treasury
			T::CDPTreasury::deposit_collateral(
				&Self::account_id(),
				T::CollateralCurrencyId::get(),
				collateral_confiscate,
			)?;

			// deposit debit to cdp treasury
			let bad_debt_value = T::RiskManager::get_debit_value(T::CollateralCurrencyId::get(), debit_decrease);
			T::CDPTreasury::on_system_debit(bad_debt_value)?;

			// update loan
			Self::update_loan(
				who,
				collateral_adjustment.saturating_neg(),
				debit_adjustment.saturating_neg(),
			)?;

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
			collateral_adjustment: Amount,
			debit_adjustment: Amount,
		) -> DispatchResult {
			// mutate collateral and debit
			// Note: if a new position, will inc consumer
			Self::update_loan(who, collateral_adjustment, debit_adjustment)?;

			let collateral_balance_adjustment = Self::balance_try_from_amount_abs(collateral_adjustment)?;
			let debit_balance_adjustment = Self::balance_try_from_amount_abs(debit_adjustment)?;
			let module_account = Self::account_id();

			if collateral_adjustment.is_positive() {
				T::Currency::transfer(
					who,
					&module_account,
					collateral_balance_adjustment,
					ExistenceRequirement::AllowDeath,
				)?;
			} else if collateral_adjustment.is_negative() {
				T::Currency::transfer(
					&module_account,
					who,
					collateral_balance_adjustment,
					ExistenceRequirement::AllowDeath,
				)?;
			}

			if debit_adjustment.is_positive() {
				// check debit cap when increase debit
				T::RiskManager::check_debit_cap(T::CollateralCurrencyId::get(), Self::total_positions().debit)?;

				// issue debit with collateral backed by cdp treasury
				T::CDPTreasury::issue_debit(
					who,
					T::RiskManager::get_debit_value(T::CollateralCurrencyId::get(), debit_balance_adjustment),
					true,
				)?;
			} else if debit_adjustment.is_negative() {
				// repay debit
				// burn debit by cdp treasury
				T::CDPTreasury::burn_debit(
					who,
					T::RiskManager::get_debit_value(T::CollateralCurrencyId::get(), debit_balance_adjustment),
				)?;
			}

			// ensure pass risk check
			let Position { collateral, debit } = Self::positions(who);
			T::RiskManager::check_position_valid(
				T::CollateralCurrencyId::get(),
				collateral,
				debit,
				collateral_adjustment.is_negative() || debit_adjustment.is_positive(),
			)?;

			Ok(())
		}

		/// transfer whole loan of `from` to `to`
		pub fn transfer_loan(from: &T::AccountId, to: &T::AccountId) -> DispatchResult {
			// get `from` position data
			let Position { collateral, debit } = Self::positions(from);

			let Position {
				collateral: to_collateral,
				debit: to_debit,
			} = Self::positions(to);
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

			// balance -> amount
			let collateral_adjustment = Self::amount_try_from_balance(collateral)?;
			let debit_adjustment = Self::amount_try_from_balance(debit)?;

			Self::update_loan(
				from,
				collateral_adjustment.saturating_neg(),
				debit_adjustment.saturating_neg(),
			)?;
			Self::update_loan(to, collateral_adjustment, debit_adjustment)?;

			Self::deposit_event(Event::TransferLoan {
				from: from.clone(),
				to: to.clone(),
			});
			Ok(())
		}

		/// mutate records of collaterals and debits
		pub fn update_loan(
			who: &T::AccountId,
			collateral_adjustment: Amount,
			debit_adjustment: Amount,
		) -> DispatchResult {
			let collateral_balance = Self::balance_try_from_amount_abs(collateral_adjustment)?;
			let debit_balance = Self::balance_try_from_amount_abs(debit_adjustment)?;

			<Positions<T>>::try_mutate_exists(who, |may_be_position| -> DispatchResult {
				let mut p = may_be_position.take().unwrap_or_default();
				let new_collateral = if collateral_adjustment.is_positive() {
					p.collateral
						.checked_add(&collateral_balance)
						.ok_or(ArithmeticError::Overflow)
				} else {
					p.collateral
						.checked_sub(&collateral_balance)
						.ok_or(ArithmeticError::Underflow)
				}?;
				let new_debit = if debit_adjustment.is_positive() {
					p.debit.checked_add(&debit_balance).ok_or(ArithmeticError::Overflow)
				} else {
					p.debit.checked_sub(&debit_balance).ok_or(ArithmeticError::Underflow)
				}?;

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
				// NOTE: but for KSM loans in Karura, the debit amount was used before,
				// and the data will been messed up, before migration or calibration,
				// it is forbidden to turn on incentives for pool LoansIncentive(KSM).
				T::OnUpdateLoan::handle(&(who.clone(), collateral_adjustment, p.collateral))?;
				p.collateral = new_collateral;
				p.debit = new_debit;

				if p.collateral.is_zero() && p.debit.is_zero() {
					// decrease account ref if zero position
					frame_system::Pallet::<T>::dec_consumers(who);

					// remove position storage if zero position
					*may_be_position = None;
				} else {
					*may_be_position = Some(p);
				}

				Ok(())
			})?;

			TotalPositions::<T>::try_mutate(|total_positions| -> DispatchResult {
				total_positions.collateral = if collateral_adjustment.is_positive() {
					total_positions
						.collateral
						.checked_add(&collateral_balance)
						.ok_or(ArithmeticError::Overflow)
				} else {
					total_positions
						.collateral
						.checked_sub(&collateral_balance)
						.ok_or(ArithmeticError::Underflow)
				}?;

				total_positions.debit = if debit_adjustment.is_positive() {
					total_positions
						.debit
						.checked_add(&debit_balance)
						.ok_or(ArithmeticError::Overflow)
				} else {
					total_positions
						.debit
						.checked_sub(&debit_balance)
						.ok_or(ArithmeticError::Underflow)
				}?;

				Ok(())
			})?;

			Self::deposit_event(Event::PositionUpdated {
				owner: who.clone(),
				collateral_adjustment,
				debit_adjustment,
			});
			Ok(())
		}

		/// Convert `Balance` to `Amount`.
		pub fn amount_try_from_balance(b: BalanceOf<T>) -> Result<Amount, Error<T>> {
			b.try_into().map_err(|_| Error::<T>::AmountConvertFailed)
		}

		/// Convert the absolute value of `Amount` to `Balance`.
		pub fn balance_try_from_amount_abs(a: Amount) -> Result<BalanceOf<T>, Error<T>> {
			a.saturating_abs()
				.try_into()
				.map_err(|_| Error::<T>::AmountConvertFailed)
		}
	}
}
