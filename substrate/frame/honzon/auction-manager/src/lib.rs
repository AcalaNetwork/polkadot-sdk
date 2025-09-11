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

//! # Auction Manager Module
//!
//! ## Overview
//!
//! Auction the assets of the system for maintain the normal operation of the
//! business. Auction types include:
//!   - `collateral auction`: sell collateral assets for getting stable currency to eliminate the
//!     system's bad debit by auction

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::unused_unit)]
#![allow(clippy::upper_case_acronyms)]
#![allow(clippy::unnecessary_unwrap)]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{pallet_prelude::*, traits::{Currency, ExistenceRequirement, fungibles}, transactional};
use frame_system::{offchain::SubmitTransaction, pallet_prelude::*};
use pallet_traits::{Auction, AuctionHandler, AuctionManager, Change, EmergencyShutdown, Rate, SwapLimit};
use scale_info::TypeInfo;
use sp_runtime::{
		offchain::{
		storage::StorageValueRef,
		storage_lock::{self, StorageLock, Time},
		Duration,
	},
	traits::{CheckedDiv, Saturating, Zero},
	transaction_validity::{
		InvalidTransaction, TransactionPriority, TransactionSource, TransactionValidity, ValidTransaction,
	},
	DispatchError, DispatchResult, FixedPointNumber, RuntimeDebug,
};
use sp_std::prelude::*;

mod mock;
mod tests;
pub mod weights;

pub use weights::WeightInfo;

pub const OFFCHAIN_WORKER_DATA: &[u8] = b"acala/auction-manager/data/";
pub const OFFCHAIN_WORKER_LOCK: &[u8] = b"acala/auction-manager/lock/";
pub const OFFCHAIN_WORKER_MAX_ITERATIONS: &[u8] = b"acala/auction-manager/max-iterations/";
pub const LOCK_DURATION: u64 = 100;
pub const DEFAULT_MAX_ITERATIONS: u32 = 1000;

pub type AuctionId = u32;

/// Information of an collateral auction
#[cfg_attr(feature = "std", derive(PartialEq, Eq))]
#[derive(Encode, Decode, Clone, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct CollateralAuctionItem<AccountId, BlockNumber, Balance: MaxEncodedLen> {
	/// Refund recipient for may receive refund
	refund_recipient: AccountId,
	/// Initial collateral amount for sale
	#[codec(compact)]
	initial_amount: Balance,
	/// Current collateral amount for sale
	#[codec(compact)]
	amount: Balance,
	/// Target sales amount of this auction
	/// if zero, collateral auction will never be reverse stage,
	/// otherwise, target amount is the actual payment amount of active
	/// bidder
	#[codec(compact)]
	target: Balance,
	/// Auction start time
	start_time: BlockNumber,
}

impl<AccountId, BlockNumber, Balance: Saturating + CheckedDiv + Copy + Ord + Zero + sp_runtime::FixedPointOperand + MaxEncodedLen>
	CollateralAuctionItem<AccountId, BlockNumber, Balance>
{
	/// Return the collateral auction will never be reverse stage
	fn always_forward(&self) -> bool {
		self.target.is_zero()
	}

	/// Return whether the collateral auction is in reverse stage at
	/// specific bid price
	fn in_reverse_stage(&self, bid_price: Balance) -> bool {
		!self.always_forward() && bid_price >= self.target
	}

	/// Return the actual number of stablecoins to be paid
	fn payment_amount(&self, bid_price: Balance) -> Balance {
		if self.always_forward() {
			bid_price
		} else {
			sp_std::cmp::min(self.target, bid_price)
		}
	}

	/// Return new collateral amount at specific last bid price and new bid
	/// price
	fn collateral_amount(&self, last_bid_price: Balance, new_bid_price: Balance) -> Balance {
		if self.in_reverse_stage(new_bid_price) && new_bid_price > last_bid_price {
			Rate::checked_from_rational(sp_std::cmp::max(last_bid_price, self.target), new_bid_price)
				.and_then(|n| n.checked_mul_int(self.amount))
				.unwrap_or(self.amount)
		} else {
			self.amount
		}
	}
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The balance type
		type Balance: frame_support::traits::tokens::Balance + MaxEncodedLen + sp_runtime::FixedPointOperand;
		
		/// The asset kind managed by this pallet.
		/// The minimum increment size of each bid compared to the previous one
		#[pallet::constant]
		type MinimumIncrementSize: Get<Rate>;

		/// The extended time for the auction to end after each successful bid
		#[pallet::constant]
		type AuctionTimeToClose: Get<BlockNumberFor<Self>>;

		/// When the total duration of the auction exceeds this soft cap, push
		/// the auction to end more faster
		#[pallet::constant]
		type AuctionDurationSoftCap: Get<BlockNumberFor<Self>>;

		type Currency: Currency<Self::AccountId, Balance = Self::Balance>;

		/// Auction to manager the auction process
		type Auction: Auction<Self::AccountId, BlockNumberFor<Self>, AuctionId = AuctionId, Balance = Self::Balance>;

		/// A configuration for base priority of unsigned transactions.
		///
		/// This is exposed so that it can be tuned for particular runtime, when
		/// multiple modules send unsigned transactions.
		#[pallet::constant]
		type UnsignedPriority: Get<TransactionPriority>;

		/// Emergency shutdown.
		type EmergencyShutdown: EmergencyShutdown;

		/// Weight information for the extrinsics in this module.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The auction dose not exist
		AuctionNotExists,
		/// The collateral auction is in reverse stage now
		InReverseStage,
		/// Feed price is invalid
		InvalidFeedPrice,
		/// Must after system shutdown
		MustAfterShutdown,
		/// Bid price is invalid
		InvalidBidPrice,
		/// Invalid input amount
		InvalidAmount,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Collateral auction created.
		NewCollateralAuction {
			auction_id: AuctionId,
			collateral_amount: T::Balance,
			target_bid_price: T::Balance,
		},
		/// Active auction cancelled.
		CancelAuction { auction_id: AuctionId },
		/// Collateral auction dealt.
		CollateralAuctionDealt {
			auction_id: AuctionId,
			collateral_amount: T::Balance,
			winner: T::AccountId,
			payment_amount: T::Balance,
		},
		/// Dex take collateral auction.
		DEXTakeCollateralAuction {
			auction_id: AuctionId,
			collateral_amount: T::Balance,
			supply_collateral_amount: T::Balance,
			target_stable_amount: T::Balance,
		},
		/// Collateral auction aborted.
		CollateralAuctionAborted {
			auction_id: AuctionId,
			collateral_amount: T::Balance,
			target_stable_amount: T::Balance,
			refund_recipient: T::AccountId,
		},
	}

	/// Mapping from auction id to collateral auction info
	///
	/// CollateralAuctions: map AuctionId => Option<CollateralAuctionItem>
	#[pallet::storage]
	#[pallet::getter(fn collateral_auctions)]
	pub type CollateralAuctions<T: Config> = StorageMap<
		_,
		Twox64Concat,
		AuctionId,
		CollateralAuctionItem<T::AccountId, BlockNumberFor<T>, T::Balance>,
		OptionQuery,
	>;

	/// Record of the total collateral amount of all active collateral auctions
	///
	/// TotalCollateralInAuction: Balance
	#[pallet::storage]
	#[pallet::getter(fn total_collateral_in_auction)]
	pub type TotalCollateralInAuction<T: Config> = StorageValue<_, T::Balance, ValueQuery>;

	/// Record of total target sales of all active collateral auctions
	///
	/// TotalTargetInAuction: Balance
	#[pallet::storage]
	#[pallet::getter(fn total_target_in_auction)]
	pub type TotalTargetInAuction<T: Config> = StorageValue<_, T::Balance, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Start offchain worker in order to submit unsigned tx to cancel
		/// active auction after system shutdown.
		fn offchain_worker(now: BlockNumberFor<T>) {
			if T::EmergencyShutdown::is_shutdown() && sp_io::offchain::is_validator() {
				// if let Err(e) = Self::_offchain_worker() {
			// 	log::info!(
			// 		target: "auction-manager",
			// 		"offchain worker: cannot run offchain worker at {:?}: {:?}",
			// 		now, e,
			// 	);
			// } else {
			// 	log::debug!(
			// 		target: "auction-manager",
			// 		"offchain worker: offchain worker start at block: {:?} already done!",
			// 		now,
			// 	);
			// }
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Cancel active auction after system shutdown
		///
		/// The dispatch origin of this call must be _None_.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::cancel_collateral_auction())]
		pub fn cancel(origin: OriginFor<T>, id: AuctionId) -> DispatchResult {
			ensure_none(origin)?;
			ensure!(T::EmergencyShutdown::is_shutdown(), Error::<T>::MustAfterShutdown);
			<Self as AuctionManager<T::AccountId>>::cancel_auction(id)?;
			Self::deposit_event(Event::CancelAuction { auction_id: id });
			Ok(())
		}
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;
		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			if let Call::cancel { id: auction_id } = call {
				if !T::EmergencyShutdown::is_shutdown() {
					return InvalidTransaction::Call.into();
				}

				if let Some(_collateral_auction) = Self::collateral_auctions(auction_id) {
					// if let Some((_, bid_price)) = Self::get_last_bid(*auction_id) {
					// 	// if collateral auction is in reverse stage, shouldn't cancel
					// 	if collateral_auction.in_reverse_stage(bid_price) {
					// 		return InvalidTransaction::Stale.into();
					// 	}
					// }
				} else {
					return InvalidTransaction::Stale.into();
				}

				ValidTransaction::with_tag_prefix("AuctionManagerOffchainWorker")
					.priority(T::UnsignedPriority::get())
					.and_provides(auction_id)
					.longevity(64_u64)
					.propagate(true)
					.build()
			} else {
				InvalidTransaction::Call.into()
			}
		}
	}
}

impl<T: pallet::Config> AuctionManager<<T as frame_system::Config>::AccountId> for pallet::Pallet<T> {
	type CurrencyId = ();
	type Balance = <T as pallet::Config>::Balance;
	type AuctionId = AuctionId;

	fn new_collateral_auction(
		_refund_recipient: &<T as frame_system::Config>::AccountId,
		_currency_id: Self::CurrencyId,
		_amount: Self::Balance,
		_target: Self::Balance,
	) -> DispatchResult {
		// TODO: implement this
		Ok(())
	}

	fn cancel_auction(_id: Self::AuctionId) -> DispatchResult {
		// TODO: implement this
		Ok(())
	}

	fn get_total_collateral_in_auction(_id: Self::CurrencyId) -> Self::Balance {
		// TODO: implement this
		Default::default()
	}

	fn get_total_target_in_auction() -> Self::Balance {
		// TODO: implement this
		Default::default()
	}
}
