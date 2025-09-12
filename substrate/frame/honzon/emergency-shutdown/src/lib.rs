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

//! # Emergency Shutdown
//!
//! ## Overview
//!
//! When a black swan event occurs, such as a severe price plunge or a critical bug, the highest
//! priority is to minimize user losses. If a decision to shut down the system is made, this
//! pallet coordinates the shutdown process. It halts related modules, freezes the collateral
//! price, and enables the settlement of all outstanding positions. Once all debts are settled and
//! auctions are resolved, stablecoin holders can redeem their stablecoins for a proportional
//! amount of the single native collateral asset.
//!
//! The emergency shutdown process consists of three main stages:
//!
//! 1.  **Shutdown:** Triggered by a privileged origin, this stage freezes the collateral price and
//!     prevents new operations.
//! 2.  **Settlement:** All outstanding CDPs are settled, and any ongoing collateral auctions are
//!     canceled or resolved.
//! 3.  **Refund:** Once the system is fully settled, stablecoin holders can burn their
//!     stablecoins to claim a proportional share of the remaining collateral.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::unused_unit)]

use frame_support::pallet_prelude::*;
use frame_system::{ensure_signed, pallet_prelude::*};
use pallet_traits::{LockablePrice, Ratio};
use pallet_traits::{AuctionManager, CDPTreasury, EmergencyShutdown};

use sp_runtime::{traits::Zero, FixedPointNumber};
use sp_std::prelude::*;

mod mock;
mod tests;
pub mod weights;

pub use pallet::*;
pub use weights::WeightInfo;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_loans::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The single collateral currency type. This should be the native currency.
		type CollateralCurrencyId: Get<<Self as pallet_loans::Config>::CurrencyId>;

		/// The price source for the collateral currency, used to freeze the price during shutdown.
		type PriceSource: LockablePrice<<Self as pallet_loans::Config>::CurrencyId>;

		/// The CDP treasury, which holds the collateral assets post-settlement.
		type CDPTreasury: CDPTreasury<Self::AccountId, Balance = pallet_loans::BalanceOf<Self>>;

		/// The auction manager, used to verify that all auctions are resolved before refunds can
		/// be processed.
		type AuctionManagerHandler: AuctionManager<
			Self::AccountId,
			Balance = pallet_loans::BalanceOf<Self>,
			CurrencyId = <Self as pallet_loans::Config>::CurrencyId,
		>;

		/// The origin that is allowed to trigger the emergency shutdown.
		type ShutdownOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The system has already been shut down.
		AlreadyShutdown,
		/// The operation can only be performed after the system has been shut down.
		MustAfterShutdown,
		/// The final refund stage has not been opened yet.
		CanNotRefund,
		/// Cannot open refunds while there is still collateral in auctions.
		ExistPotentialSurplus,
		/// Cannot open refunds while there are still outstanding debts.
		ExistUnhandledDebit,
	}

	#[pallet::event]
	#[pallet::generate_deposit(fn deposit_event)]
	pub enum Event<T: Config> {
		/// The system has entered emergency shutdown.
		Shutdown {
			/// The block number when the shutdown was triggered.
			block_number: BlockNumberFor<T>,
		},
		/// The final refund stage has been opened.
		OpenRefund {
			/// The block number when the refund stage was opened.
			block_number: BlockNumberFor<T>,
		},
		/// A user has refunded their stablecoin for collateral.
		Refund {
			/// The account that performed the refund.
			who: T::AccountId,
			/// The amount of stablecoin burned.
			stable_coin_amount: pallet_loans::BalanceOf<T>,
			/// The ID of the refunded collateral currency.
			refunded_collateral_currency_id: <T as pallet_loans::Config>::CurrencyId,
			/// The amount of collateral refunded.
			refunded_collateral_amount: pallet_loans::BalanceOf<T>,
		},
	}

	/// A flag indicating whether the emergency shutdown process has been initiated.
	///
	/// `true` if the system is in shutdown, `false` otherwise.
	#[pallet::storage]
	#[pallet::getter(fn is_shutdown)]
	pub type IsShutdown<T: Config> = StorageValue<_, bool, ValueQuery>;

	/// A flag indicating whether the final refund stage has been opened.
	///
	/// `true` if refunds are allowed, `false` otherwise.
	#[pallet::storage]
	#[pallet::getter(fn can_refund)]
	pub type CanRefund<T: Config> = StorageValue<_, bool, ValueQuery>;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Initiates an emergency shutdown of the system.
		///
		/// This extrinsic freezes the collateral price and transitions the system to a shutdown
		/// state, preventing new operations.
		///
		/// The dispatch origin of this call must be `ShutdownOrigin`.
		#[pallet::call_index(0)]
		#[pallet::weight((T::WeightInfo::emergency_shutdown(1), DispatchClass::Operational))]
		pub fn emergency_shutdown(origin: OriginFor<T>) -> DispatchResult {
			T::ShutdownOrigin::ensure_origin(origin)?;
			ensure!(!Self::is_shutdown(), Error::<T>::AlreadyShutdown);

			// get all collateral types
			let currency_id = <T as pallet::Config>::CollateralCurrencyId::get();

			// lock price for every collateral
			// TODO: check the results
			let _ = <T as Config>::PriceSource::lock_price(currency_id);

			IsShutdown::<T>::put(true);
			Self::deposit_event(Event::Shutdown {
				block_number: <frame_system::Pallet<T>>::block_number(),
			});
			Ok(())
		}

		/// Enables the final refund stage once the system is fully settled.
		///
		/// This can only be called after the system has been shut down and all outstanding debts
		/// and auctions have been resolved.
		///
		/// The dispatch origin of this call must be `ShutdownOrigin`.
		#[pallet::call_index(1)]
		#[pallet::weight((T::WeightInfo::open_collateral_refund(), DispatchClass::Operational))]
		pub fn open_collateral_refund(origin: OriginFor<T>) -> DispatchResult {
			T::ShutdownOrigin::ensure_origin(origin)?;
			ensure!(Self::is_shutdown(), Error::<T>::MustAfterShutdown); // must after shutdown

			// Ensure all debits of CDPs have been settled, and all collateral auction has
			// been done or canceled. Settle all collaterals type CDPs which have debit,
			// cancel all collateral auctions in forward stage and wait for all collateral
			// auctions in reverse stage to be ended.
			let currency_id = <T as pallet::Config>::CollateralCurrencyId::get();
			// there's no collateral auction
			ensure!(
				<T as Config>::AuctionManagerHandler::get_total_collateral_in_auction(currency_id).is_zero(),
				Error::<T>::ExistPotentialSurplus,
			);
			// there's on debit in CDP
			ensure!(
				pallet_loans::Pallet::<T>::total_positions().debit.is_zero(),
				Error::<T>::ExistUnhandledDebit,
			);

			// Open refund stage
			CanRefund::<T>::put(true);
			Self::deposit_event(Event::OpenRefund {
				block_number: <frame_system::Pallet<T>>::block_number(),
			});
			Ok(())
		}

		/// Refunds a proportional amount of the single native collateral in exchange for the
		/// stablecoin.
		///
		/// This is only available after the refund stage has been opened.
		///
		/// - `amount`: The amount of stablecoin to be burned in exchange for collateral.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::refund_collaterals(1))]
		pub fn refund_collaterals(
			origin: OriginFor<T>,
			#[pallet::compact] amount: pallet_loans::BalanceOf<T>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			ensure!(Self::can_refund(), Error::<T>::CanNotRefund);

			let refund_ratio: Ratio = <T as Config>::CDPTreasury::get_debit_proportion(amount);
			let currency_id = <T as pallet::Config>::CollateralCurrencyId::get();

			// burn caller's stable currency by CDP treasury
			<T as Config>::CDPTreasury::burn_debit(&who, amount)?;

			// refund collaterals to caller by CDP treasury
			let refund_amount =
				refund_ratio.saturating_mul_int(<T as Config>::CDPTreasury::get_total_collaterals());

			if !refund_amount.is_zero() {
				<T as Config>::CDPTreasury::withdraw_collateral(&who, refund_amount)?;
			}

			Self::deposit_event(Event::Refund {
				who,
				stable_coin_amount: amount,
				refunded_collateral_currency_id: currency_id,
				refunded_collateral_amount: refund_amount,
			});
			Ok(())
		}
	}
}

impl<T: Config> EmergencyShutdown for Pallet<T> {
	fn is_shutdown() -> bool {
		Self::is_shutdown()
	}
}
