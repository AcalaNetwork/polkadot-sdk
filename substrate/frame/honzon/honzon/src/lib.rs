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

//! # Honzon Pallet
//!
//! ## Overview
//!
//! This pallet is the entry point for users to manage their Collateralized Debt Positions (now
//! called `Position`s). It allows users to borrow a stablecoin against a collateral asset.
//! Currently, the native currency is the only supported collateral type.
//!
//! This pallet is built on top of the following pallets:
//! - `pallet-cdp-engine`: Manages the underlying mechanics of `Position`s, including liquidation
//!   and stability fees.
//! - `pallet-loans`: Handles the accounting of loans and collateral.
//! - `pallet-oracle`: Provides the price feeds for collateral assets.
//!
//! ### Key Functions
//!
//! - `adjust_loan`: Allows users to adjust their `Position` by depositing or withdrawing collateral
//!   and borrowing or repaying the stablecoin.
//! - `close_loan_has_debit_by_dex`: Allows users to close their `Position` by selling a portion of
//!   their collateral on a DEX to repay the outstanding debt.
//!
//! ### Not Yet Implemented
//!
//! The following functions are not yet implemented:
//! - `expand_position_collateral`
//! - `shrink_position_debit`
//! - `close_loan_has_debit_by_dex`
//!
//! After a system shutdown, some operations will be restricted.

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
	pallet_prelude::*,
	traits::{
		honzon::{
			EmergencyShutdown, ExchangeRate, HonzonManager, Position, PriceProvider, Rate, Ratio,
		},
		Get, NamedReservableCurrency,
	},
};
use frame_system::pallet_prelude::*;
use pallet_loans::BalanceOf;
use sp_core::U256;
use sp_runtime::{traits::StaticLookup, DispatchResult};
use sp_std::prelude::*;

mod mock;
mod tests;
pub mod weights;

pub use pallet::*;
pub use weights::WeightInfo;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	pub type CurrencyId<T> = <T as pallet_loans::Config>::CurrencyId;
	pub type BalanceAdjustmentOf<T> = pallet_loans::BalanceAdjustment<pallet_loans::BalanceOf<T>>;

	pub type ReserveIdentifier = [u8; 8];
	pub const RESERVE_ID: ReserveIdentifier = *b"honzon  ";

	#[pallet::config]
	pub trait Config:
		frame_system::Config + pallet_cdp_engine::Config + pallet_loans::Config
	{
		/// Currency for authorization reserved.
		type Currency: NamedReservableCurrency<
			Self::AccountId,
			Balance = BalanceOf<Self>,
			ReserveIdentifier = ReserveIdentifier,
		>;

		/// Reserved amount per authorization.
		#[pallet::constant]
		type DepositPerAuthorization: Get<BalanceOf<Self>>;

		/// The collateral currency id
		type CollateralCurrencyId: Get<CurrencyId<Self>>;

		/// Weight information for the extrinsics in this module.
		type WeightInfo: WeightInfo;

		/// Emergency shutdown manager
		type EmergencyShutdown: EmergencyShutdown;

		/// Price provider
		type PriceSource: PriceProvider<CurrencyId<Self>>;

		/// Stable currency id
		type GetStableCurrencyId: Get<CurrencyId<Self>>;

		/// The origin which can update stability fee.
		type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;
	}

	#[pallet::error]
	pub enum Error<T> {
		// No permission
		NoPermission,
		// The system has been shutdown
		AlreadyShutdown,
	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Adjusts a `Position` by changing the collateral and debit amounts.
		///
		/// - `collateral_adjustment`: A signed amount representing the change in collateral. A
		///   positive value deposits collateral, while a negative value withdraws it.
		/// - `debit_adjustment`: A signed amount representing the change in debit. A positive value
		///   issues more stablecoin to the caller, while a negative value represents a repayment.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::adjust_loan())]
		pub fn adjust_loan(
			origin: OriginFor<T>,
			collateral_adjustment: BalanceAdjustmentOf<T>,
			debit_adjustment: BalanceAdjustmentOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Self::do_adjust_loan(&who, collateral_adjustment, debit_adjustment)
		}
		/// Closes a `Position` that has outstanding debit by selling collateral on a DEX.
		///
		/// This function is used when the `Position` is still in a safe state (i.e., not
		/// subject to liquidation).
		///
		/// - `max_collateral_amount`: The maximum amount of collateral to be sold to swap for the
		///   stablecoin needed to clear the debit.
		///
		/// **Note:** This function is not yet implemented.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::close_loan_has_debit_by_dex())]
		pub fn close_loan_has_debit_by_dex(
			origin: OriginFor<T>,
			max_collateral_amount: BalanceOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Self::do_close_loan_by_dex(who, max_collateral_amount)
		}

		/// Generates new debit in advance, buys collateral, and deposits it into the `Position`.
		///
		/// - `increase_debit_value`: The specific increased debit value for the `Position`.
		/// - `min_increase_collateral`: The minimum amount of collateral to be added to the
		///   `Position`.
		///
		/// **Note:** This function is not yet implemented.
		#[pallet::call_index(6)]
		#[pallet::weight(<T as Config>::WeightInfo::expand_position_collateral())]
		pub fn expand_position_collateral(
			origin: OriginFor<T>,
			_increase_debit_value: BalanceOf<T>,
			_min_increase_collateral: BalanceOf<T>,
		) -> DispatchResult {
			let _who = ensure_signed(origin)?;
			// TODO: not implemented
			Ok(())
		}

		/// Sells the collateral locked in a `Position` to get stablecoin to repay the debit.
		///
		/// - `decrease_collateral`: The specific amount of collateral to be sold from the
		///   `Position`.
		/// - `min_decrease_debit_value`: The minimum amount of debit to be repaid.
		///
		/// **Note:** This function is not yet implemented.
		#[pallet::call_index(7)]
		#[pallet::weight(<T as Config>::WeightInfo::shrink_position_debit())]
		pub fn shrink_position_debit(
			origin: OriginFor<T>,
			_decrease_collateral: BalanceOf<T>,
			_min_decrease_debit_value: BalanceOf<T>,
		) -> DispatchResult {
			let _who = ensure_signed(origin)?;
			// TODO: not implemented
			Ok(())
		}

		/// Adjusts a `Position` by changing the collateral and debit amounts, where the debit
		/// adjustment is specified as a value in terms of the stablecoin.
		///
		/// This differs from `adjust_loan` where the debit adjustment is a direct amount of
		/// the stablecoin.
		///
		/// - `collateral_adjustment`: A signed amount representing the change in collateral. A
		///   positive value deposits collateral, while a negative value withdraws it.
		/// - `debit_value_adjustment`: A signed amount representing the change in the debit's
		///   value. A positive value issues more stablecoin, while a negative value represents a
		///   repayment.
		///
		/// **Note:** This function is not yet implemented.
		#[pallet::call_index(8)]
		#[pallet::weight(<T as Config>::WeightInfo::adjust_loan())]
		pub fn adjust_loan_by_debit_value(
			origin: OriginFor<T>,
			collateral_adjustment: BalanceAdjustmentOf<T>,
			debit_value_adjustment: BalanceAdjustmentOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// not allowed to adjust the debit after system shutdown
			if !debit_value_adjustment.is_zero() {
				ensure!(
					!<T as crate::Config>::EmergencyShutdown::is_shutdown(),
					Error::<T>::AlreadyShutdown
				);
			}

			<pallet_cdp_engine::Pallet<T>>::adjust_position_by_debit_value(
				&who,
				collateral_adjustment,
				debit_value_adjustment,
			)
		}

		/// Updates the stability fee for a specific `Position`.
		///
		/// - `who`: The account whose `Position` is to be updated.
		/// - `new_stability_fee`: The new stability fee to be set.
		///
		/// This extrinsic can only be called by `AdminOrigin`.
		#[pallet::call_index(9)]
		#[pallet::weight(<T as Config>::WeightInfo::set_stability_fee())]
		pub fn set_stability_fee(
			origin: OriginFor<T>,
			who: <T::Lookup as StaticLookup>::Source,
			new_stability_fee: Rate,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin(origin)?;
			let who = T::Lookup::lookup(who)?;

			<pallet_loans::Pallet<T>>::adjust_position(
				&who,
				pallet_loans::BalanceAdjustment::Increase(0u32.into()),
				pallet_loans::BalanceAdjustment::Increase(0u32.into()),
				Some(new_stability_fee),
			)?;

			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	fn do_adjust_loan(
		who: &<T as frame_system::Config>::AccountId,
		collateral_adjustment: BalanceAdjustmentOf<T>,
		debit_adjustment: BalanceAdjustmentOf<T>,
	) -> DispatchResult {
		// not allowed to adjust the debit after system shutdown
		if !debit_adjustment.is_zero() {
			ensure!(
				!<T as crate::Config>::EmergencyShutdown::is_shutdown(),
				Error::<T>::AlreadyShutdown
			);
		}
		let maybe_new_stability_fee = if debit_adjustment.is_increase() {
			Some(<pallet_cdp_engine::Pallet<T>>::get_interest_rate_per_sec()?)
		} else {
			None
		};
		<pallet_loans::Pallet<T>>::adjust_position(
			who,
			collateral_adjustment,
			debit_adjustment,
			maybe_new_stability_fee,
		)?;
		Ok(())
	}

	fn do_close_loan_by_dex(
		_who: <T as frame_system::Config>::AccountId,
		_max_collateral_amount: BalanceOf<T>,
	) -> DispatchResult {
		ensure!(
			!<T as crate::Config>::EmergencyShutdown::is_shutdown(),
			Error::<T>::AlreadyShutdown
		);
		// TODO: not implemented
		Ok(())
	}
}

impl<T: Config>
	HonzonManager<<T as frame_system::Config>::AccountId, BalanceAdjustmentOf<T>, BalanceOf<T>>
	for Pallet<T>
{
	fn adjust_loan(
		who: &<T as frame_system::Config>::AccountId,
		collateral_adjustment: BalanceAdjustmentOf<T>,
		debit_adjustment: BalanceAdjustmentOf<T>,
	) -> DispatchResult {
		Self::do_adjust_loan(who, collateral_adjustment, debit_adjustment)
	}

	fn close_loan_by_dex(
		who: <T as frame_system::Config>::AccountId,
		max_collateral_amount: BalanceOf<T>,
	) -> DispatchResult {
		Self::do_close_loan_by_dex(who, max_collateral_amount)
	}

	fn get_position(who: &<T as frame_system::Config>::AccountId) -> Position<BalanceOf<T>> {
		<pallet_loans::Pallet<T>>::positions(who)
	}

	fn get_collateral_parameters() -> Vec<U256> {
		let params = <pallet_cdp_engine::Pallet<T>>::collateral_params();

		vec![
			Ratio::one().into_inner().into(),
			params.liquidation_ratio.unwrap_or_default().into_inner().into(),
			Ratio::one().into_inner().into(),
			params.required_collateral_ratio.unwrap_or_default().into_inner().into(),
		]
	}

	fn get_current_collateral_ratio(who: &<T as frame_system::Config>::AccountId) -> Option<Ratio> {
		let currency_id = <T as crate::Config>::CollateralCurrencyId::get();
		let position: Position<BalanceOf<T>> = <pallet_loans::Pallet<T>>::positions(who);
		let stable_currency_id = <T as crate::Config>::GetStableCurrencyId::get();
		let stability_fee = Rate::from_inner(position.stability_fee.into_inner());

		<T as crate::Config>::PriceSource::get_relative_price(currency_id, stable_currency_id).map(
			|price| {
				let exchange_rate =
					<pallet_cdp_engine::Pallet<T>>::get_debit_exchange_rate(stability_fee);
				<pallet_cdp_engine::Pallet<T>>::calculate_collateral_ratio(
					position.collateral.into(),
					position.debit.into(),
					price,
					exchange_rate,
				)
			},
		)
	}

	fn get_debit_exchange_rate() -> ExchangeRate {
		ExchangeRate::one()
	}
}
