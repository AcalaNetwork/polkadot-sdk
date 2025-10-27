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

//! # Auction Manager Pallet
//!
//! ## Overview
//!
//! The Auction Manager pallet is responsible for managing auctions of system assets to ensure the
//! normal operation of the business. It handles collateral auctions, which involve selling
//! collateral assets to acquire stable currency and cover the system's bad debt.
//!
//! This pallet implements the `AuctionManager` and `AuctionHandler` traits, providing a structured
//! way to create, manage, and settle auctions. It interacts with other pallets like
//! `pallet-auction` for the core auction mechanics and `pallet-cdp-treasury` for handling funds.

#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::{
	pallet_prelude::*,
	traits::{
		fungibles,
		honzon::{CDPTreasury, EmergencyShutdown, PriceProvider, Rate},
	},
};
use frame_system::pallet_prelude::*;

mod impl_auction_handler;
mod impl_auction_manager;
mod mock;
mod tests;
pub mod types;
pub use types::CollateralAuctionItem;

pub mod weights;
pub use weights::WeightInfo;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(sp_std::marker::PhantomData<T>);

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_auction::Config
	where
		<Self as pallet_auction::Config>::Balance: Zero,
	{
		/// The currency type for interacting with this pallet.
		type CurrencyId: frame_support::pallet_prelude::Parameter
			+ PartialEq
			+ Eq
			+ Copy
			+ Ord
			+ Send
			+ Sync
			+ MaxEncodedLen
			+ 'static;
		/// The native currency ID.
		type GetNativeCurrencyId: Get<Self::CurrencyId>;
		/// The stable currency ID.
		type GetStableCurrencyId: Get<Self::CurrencyId>;
		/// The CDP treasury pallet.
		type CDPTreasury: CDPTreasury<
			Self::AccountId,
			Balance = <Self as pallet_auction::Config>::Balance,
			CurrencyId = Self::CurrencyId,
		>;
		/// The price provider.
		type PriceSource: PriceProvider<Self::CurrencyId>;
		/// The hold reason for this pallet.
		type RuntimeHoldReason: From<HoldReason>;
		/// The minimum increment size for bids in an auction.
		#[pallet::constant]
		type MinimumIncrementSize: Get<Rate>;
		/// The time to close an auction.
		#[pallet::constant]
		type AuctionTimeToClose: Get<BlockNumberFor<Self>>;
		/// The soft cap for auction duration.
		#[pallet::constant]
		type AuctionDurationSoftCap: Get<BlockNumberFor<Self>>;
		/// The currency handler.
		type Fungibles: fungibles::Mutate<
				Self::AccountId,
				AssetId = Self::CurrencyId,
				Balance = <Self as pallet_auction::Config>::Balance,
			> + fungibles::Balanced<
				Self::AccountId,
				AssetId = Self::CurrencyId,
				Balance = <Self as pallet_auction::Config>::Balance,
			> + fungibles::hold::Mutate<
				Self::AccountId,
				AssetId = Self::CurrencyId,
				Balance = <Self as pallet_auction::Config>::Balance,
				Reason = Self::RuntimeHoldReason,
			>;
		/// The emergency shutdown pallet.
		type EmergencyShutdown: EmergencyShutdown;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	/// Reasons for holding funds in this pallet.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Funds are held for a collateral auction.
		CollateralAuction,
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The specified auction does not exist.
		AuctionNotExists,
		/// The auction is in the reverse stage and cannot be canceled.
		InReverseStage,
		/// The operation can only be performed after an emergency shutdown.
		MustAfterShutdown,
		/// The bid price is invalid.
		InvalidBidPrice,
		/// The amount is invalid.
		InvalidAmount,
		/// The currency ID is invalid.
		InvalidCurrencyId,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new collateral auction has been created.
		NewCollateralAuction {
			/// The ID of the auction.
			auction_id: <T as pallet_auction::Config>::AuctionId,
			/// The type of collateral being auctioned.
			collateral_type: T::CurrencyId,
			/// The amount of collateral being auctioned.
			collateral_amount: <T as pallet_auction::Config>::Balance,
			/// The target bid price for the auction.
			target_bid_price: <T as pallet_auction::Config>::Balance,
		},
		/// An auction has been canceled.
		CancelAuction {
			/// The ID of the canceled auction.
			auction_id: <T as pallet_auction::Config>::AuctionId,
		},
		/// A collateral auction has been successfully dealt.
		CollateralAuctionDealt {
			/// The ID of the auction.
			auction_id: <T as pallet_auction::Config>::AuctionId,
			/// The type of collateral that was auctioned.
			collateral_type: T::CurrencyId,
			/// The amount of collateral that was auctioned.
			collateral_amount: <T as pallet_auction::Config>::Balance,
			/// The winner of the auction.
			winner: T::AccountId,
			/// The amount paid by the winner.
			payment_amount: <T as pallet_auction::Config>::Balance,
		},
		/// A collateral auction has been aborted due to no bids.
		CollateralAuctionAborted {
			/// The ID of the auction.
			auction_id: <T as pallet_auction::Config>::AuctionId,
			/// The type of collateral that was being auctioned.
			collateral_type: T::CurrencyId,
			/// The amount of collateral that was being auctioned.
			collateral_amount: <T as pallet_auction::Config>::Balance,
			/// The target stable amount for the auction.
			target_stable_amount: <T as pallet_auction::Config>::Balance,
			/// The recipient of the refunded collateral.
			refund_recipient: T::AccountId,
		},
	}

	/// Stores the details of each collateral auction, indexed by auction ID.
	#[pallet::storage]
	#[pallet::getter(fn collateral_auctions)]
	pub type CollateralAuctions<T: Config> = StorageMap<
		_,
		Twox64Concat,
		<T as pallet_auction::Config>::AuctionId,
		CollateralAuctionItem<
			T::AccountId,
			BlockNumberFor<T>,
			<T as pallet_auction::Config>::Balance,
		>,
		OptionQuery,
	>;

	/// The total amount of collateral currently in auction.
	#[pallet::storage]
	#[pallet::getter(fn total_collateral_in_auction)]
	pub type TotalCollateralInAuction<T: Config> =
		StorageValue<_, <T as pallet_auction::Config>::Balance, ValueQuery>;

	/// The total target amount to be raised from all auctions.
	#[pallet::storage]
	#[pallet::getter(fn total_target_in_auction)]
	pub type TotalTargetInAuction<T: Config> =
		StorageValue<_, <T as pallet_auction::Config>::Balance, ValueQuery>;

	impl<T: Config> Pallet<T> {
		/// Determines the remaining time of closing for a given auction
		///
		/// If the auction is still within the soft cap period, it uses the full closing time.
		/// After the soft cap has elapsed, the closing period is reduced by half to encourage a
		/// quicker auction end.
		///
		/// # Parameters
		/// - `auction_start_time`: The block number when the auction started.
		/// - `now`: The current block number.
		///
		/// # Returns
		/// - `BlockNumberFor<T>`: The time to close (in blocks) for the auction.
		pub fn get_auction_time_to_close(
			auction_start_time: BlockNumberFor<T>,
			now: BlockNumberFor<T>,
		) -> BlockNumberFor<T> {
			let auction_duration_soft_cap = T::AuctionDurationSoftCap::get();
			let time_to_close = T::AuctionTimeToClose::get();

			// If we are past the soft cap, reduce the time to close by half;
			// otherwise, use the full auction time to close.
			if now >= auction_start_time + auction_duration_soft_cap {
				time_to_close / 2u32.into()
			} else {
				time_to_close
			}
		}
	}
}
