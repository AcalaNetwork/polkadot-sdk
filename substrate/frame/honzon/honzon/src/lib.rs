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

//! # Honzon Module
//!
//! ## Overview
//!
//! The entry of the Honzon protocol for users, user can manipulate their CDP
//! position to loan/payback`
//!
//! After system shutdown, some operations will be restricted.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::unused_unit)]

use codec::{EncodeLike, FullCodec, MaxEncodedLen};
use core::fmt::Debug;
use frame_support::{
	pallet_prelude::*,
	traits::{Get, NamedReservableCurrency},
};
use sp_core::U256;
use frame_system::pallet_prelude::*;
use pallet_loans::{Amount, BalanceOf};
use pallet_traits::{EmergencyShutdown, ExchangeRate, HonzonManager, Position, PriceProvider, Ratio};
use sp_runtime::{
	traits::{AtLeast32BitUnsigned, MaybeSerializeDeserialize, StaticLookup, Zero},
	ArithmeticError, DispatchResult,
};
use sp_std::prelude::*;

mod mock;
mod tests;
pub mod weights;

pub use pallet::*;
pub use weights::WeightInfo;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	pub type ReserveIdentifier = [u8; 8];
	pub const RESERVE_ID: ReserveIdentifier = *b"honzon  ";

	#[pallet::config]
	pub trait Config:
		frame_system::Config
		+ pallet_cdp_engine::Config
		+ pallet_loans::Config
	{
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Currency for authorization reserved.
		type Currency: NamedReservableCurrency<
			Self::AccountId,
			Balance = BalanceOf<Self>,
			ReserveIdentifier = ReserveIdentifier,
		>;

		/// Reserved amount per authorization.
		#[pallet::constant]
		type DepositPerAuthorization: Get<<Self as pallet_cdp_engine::Config>::Balance>;

		/// The collateral currency id
		type CollateralCurrencyId: Get<<Self as pallet_loans::Config>::CurrencyId>;

		/// Weight information for the extrinsics in this module.
		type WeightInfo: WeightInfo;

		/// Emergency shutdown manager
		type EmergencyShutdown: EmergencyShutdown;

		/// Price provider
		type PriceSource: PriceProvider<<Self as pallet_loans::Config>::CurrencyId>;

		/// Stable currency id
		type GetStableCurrencyId: Get<<Self as pallet_loans::Config>::CurrencyId>;
	}

	#[pallet::error]
	pub enum Error<T> {
		// No permission
		NoPermission,
		// The system has been shutdown
		AlreadyShutdown,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config>
	{

	}

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
	{}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		BalanceOf<T>: TryFrom<i128>,
		i128: TryFrom<BalanceOf<T>>,
	{
		/// Adjust the loans by specific `collateral_adjustment` and
		/// `debit_adjustment`
		///
		/// - `collateral_adjustment`: signed amount, positive means to deposit collateral currency
		///   into CDP, negative means withdraw collateral currency from CDP.
		/// - `debit_adjustment`: signed amount, positive means to issue some amount of stablecoin
		///   to caller according to the debit adjustment, negative means caller will payback some
		///   amount of stablecoin to CDP according to the debit adjustment.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::adjust_loan())]
		pub fn adjust_loan(
			origin: OriginFor<T>,
			collateral_adjustment: Amount,
			debit_adjustment: Amount,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Self::do_adjust_loan(&who, collateral_adjustment, debit_adjustment)
		}

		/// Close caller's CDP which has debit but still in safe by use collateral to swap
		/// stable token on DEX for clearing debit.
		///
		/// - `max_collateral_amount`: the max collateral amount which is used to swap enough
		/// 	stable token to clear debit.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::close_loan_has_debit_by_dex())]
		pub fn close_loan_has_debit_by_dex(
			origin: OriginFor<T>,
			#[pallet::compact] max_collateral_amount: T::Balance,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Self::do_close_loan_by_dex(who, max_collateral_amount)
		}

		/// Generate new debit in advance, buy collateral and deposit it into CDP.
		///
		/// - `increase_debit_value`: the specific increased debit value for CDP
		/// - `min_increase_collateral`: the minimal increased collateral amount for CDP
		#[pallet::call_index(6)]
		#[pallet::weight(<T as Config>::WeightInfo::expand_position_collateral())]
		pub fn expand_position_collateral(
			origin: OriginFor<T>,
			increase_debit_value: T::Balance,
			min_increase_collateral: T::Balance,
		) -> DispatchResult {
			let _who = ensure_signed(origin)?;
			// TODO: not implemented
			Ok(())
		}

		/// Sell the collateral locked in CDP to get stable coin to repay the debit.
		///
		/// - `decrease_collateral`: the specific decreased collateral amount for CDP
		/// - `min_decrease_debit_value`: the minimal decreased debit value for CDP
		#[pallet::call_index(7)]
		#[pallet::weight(<T as Config>::WeightInfo::shrink_position_debit())]
		pub fn shrink_position_debit(
			origin: OriginFor<T>,
			decrease_collateral: T::Balance,
			min_decrease_debit_value: T::Balance,
		) -> DispatchResult {
			let _who = ensure_signed(origin)?;
			// TODO: not implemented
			Ok(())
		}

		/// Adjust the loans by specific `collateral_adjustment` and
		/// `debit_value_adjustment`
		///
		/// - `collateral_adjustment`: signed amount, positive means to deposit collateral currency
		///   into CDP, negative means withdraw collateral currency from CDP.
		/// - `debit_value_adjustment`: signed amount, positive means to issue some amount of
		///   stablecoin, negative means caller will payback some amount of stablecoin to CDP.
		#[pallet::call_index(8)]
		#[pallet::weight(<T as Config>::WeightInfo::adjust_loan())]
		pub fn adjust_loan_by_debit_value(
			origin: OriginFor<T>,
			collateral_adjustment: Amount,
			debit_value_adjustment: Amount,
		) -> DispatchResult {
			let _who = ensure_signed(origin)?;

			// not allowed to adjust the debit after system shutdown
			if !debit_value_adjustment.is_zero() {
				ensure!(!T::EmergencyShutdown::is_shutdown(), Error::<T>::AlreadyShutdown);
			}
			// TODO: not implemented
			Ok(())
		}
	}
}

	impl<T: Config> Pallet<T>
	where
		BalanceOf<T>: TryFrom<i128>,
		i128: TryFrom<BalanceOf<T>>,
	{
	fn do_adjust_loan(
		who: &<T as frame_system::Config>::AccountId,
		collateral_adjustment: Amount,
		debit_adjustment: Amount,
	) -> DispatchResult {
		// not allowed to adjust the debit after system shutdown
		if !debit_adjustment.is_zero() {
			ensure!(!T::EmergencyShutdown::is_shutdown(), Error::<T>::AlreadyShutdown);
		}
		<pallet_loans::Pallet<T>>::adjust_position(who, collateral_adjustment, debit_adjustment)?;
		Ok(())
	}

	fn do_close_loan_by_dex(
		who: <T as frame_system::Config>::AccountId,
		max_collateral_amount: T::Balance,
	) -> DispatchResult {
		ensure!(!T::EmergencyShutdown::is_shutdown(), Error::<T>::AlreadyShutdown);
		// TODO: not implemented
		Ok(())
	}
}

impl<T: Config> HonzonManager<<T as frame_system::Config>::AccountId, Amount, <T as pallet_cdp_engine::Config>::Balance> for Pallet<T>
where
	<T as pallet_cdp_engine::Config>::Balance: From<BalanceOf<T>>,
	U256: From<<T as pallet_cdp_engine::Config>::Balance>,
	BalanceOf<T>: TryFrom<i128>,
	i128: TryFrom<BalanceOf<T>>,
{
	fn adjust_loan(
		who: &<T as frame_system::Config>::AccountId,
		collateral_adjustment: Amount,
		debit_adjustment: Amount,
	) -> DispatchResult {
		Self::do_adjust_loan(who, collateral_adjustment, debit_adjustment)
	}

	fn close_loan_by_dex(who: <T as frame_system::Config>::AccountId, max_collateral_amount: <T as pallet_cdp_engine::Config>::Balance) -> DispatchResult {
		Self::do_close_loan_by_dex(who, max_collateral_amount)
	}

	fn get_position(who: &<T as frame_system::Config>::AccountId) -> pallet_traits::Position<<T as pallet_cdp_engine::Config>::Balance> {
		let position: pallet_traits::Position<BalanceOf<T>> = <pallet_loans::Pallet<T>>::positions(who);
		pallet_traits::Position {
			collateral: position.collateral.into(),
			debit: position.debit.into(),
		}
	}

	fn get_collateral_parameters() -> Vec<U256> {
		let params = <pallet_cdp_engine::Pallet<T>>::collateral_params().unwrap_or_default();

		vec![
			params.maximum_total_debit_value.into(),
			Ratio::one().into_inner().into(),
			params.liquidation_ratio.unwrap_or_default().into_inner().into(),
			Ratio::one().into_inner().into(),
			params.required_collateral_ratio.unwrap_or_default().into_inner().into(),
		]
	}

	fn get_current_collateral_ratio(who: &<T as frame_system::Config>::AccountId) -> Option<Ratio> {
		let currency_id = <T as crate::Config>::CollateralCurrencyId::get();
		let position: pallet_traits::Position<BalanceOf<T>> = <pallet_loans::Pallet<T>>::positions(who);
		let stable_currency_id = T::GetStableCurrencyId::get();

		T::PriceSource::get_relative_price(currency_id, stable_currency_id).map(|price| {
			<pallet_cdp_engine::Pallet<T>>::calculate_collateral_ratio(position.collateral.into(), position.debit.into())
		})
	}

	fn get_debit_exchange_rate() -> ExchangeRate {
		ExchangeRate::one()
	}
}
