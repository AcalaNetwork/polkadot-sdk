//! # Auction Pallet
//!
//! ## Overview
//!
//! This pallet provides a generic framework for on-chain auctions. It allows for the creation
//! and management of auctions for any type of asset. The core logic of the auction, such as
//! bid validation and what happens when an auction ends, is customizable through the
//! `AuctionHandler` trait.
//!
//! This pallet is designed to be flexible and can be used to implement various auction
//! types, such as English auctions, Dutch auctions, or other custom formats.
//!
//! ## Features
//!
//! - **Generic Auction Mechanism:** Can be used for auctioning any asset.
//! - **Customizable Logic:** The `AuctionHandler` trait allows for custom implementation of
//!   auction logic.
//! - **Scheduled Auctions:** Auctions can be scheduled to start at a future block number.
//! - **Automatic Auction Closing:** Auctions are automatically closed at their end block number
//!   in the `on_finalize` hook.
//!
//! ## Terminology
//!
//! - **Auction:** A process of buying and selling goods or services by offering them up for
//!   bid, taking bids, and then selling the item to the highest bidder.
//! - **Bid:** An offer of a price.
//! - **Auction Handler:** A trait implementation that defines the specific logic for an
//!   auction, such as how to handle new bids and what to do when an auction ends.

#![cfg_attr(not(feature = "std"), no_std)]
// Disable the following two lints since they originate from an external macro (namely decl_storage)
#![allow(clippy::string_lit_as_bytes)]
#![allow(clippy::unused_unit)]

use frame_support::pallet_prelude::*;
use frame_system::{ensure_signed, pallet_prelude::*};
use pallet_traits::{Auction, AuctionHandler, AuctionInfo, Change};
use codec::MaxEncodedLen;
use sp_runtime::{
	traits::{AtLeast32BitUnsigned, Bounded, CheckedAdd, MaybeSerializeDeserialize, Member, One, Zero},
	DispatchError, DispatchResult,
};

mod mock;
mod tests;
mod weights;

pub use module::*;
pub use weights::WeightInfo;

#[frame_support::pallet]
pub mod module {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The balance type for bidding in auctions.
		type Balance: Parameter
			+ Member
			+ AtLeast32BitUnsigned
			+ Default
			+ Copy
			+ MaybeSerializeDeserialize
			+ MaxEncodedLen;

		/// The type for identifying auctions.
		type AuctionId: Parameter
			+ Member
			+ AtLeast32BitUnsigned
			+ Default
			+ Copy
			+ MaybeSerializeDeserialize
			+ Bounded
			+ codec::FullCodec
			+ codec::MaxEncodedLen;

		/// The handler for custom auction logic. This is used to validate bids
		/// and handle the outcome of an auction.
		type Handler: AuctionHandler<Self::AccountId, Self::Balance, BlockNumberFor<Self>, Self::AuctionId>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The specified auction does not exist.
		AuctionNotExist,
		/// The auction has not started yet.
		AuctionNotStarted,
		/// The bid was not accepted by the `AuctionHandler`.
		BidNotAccepted,
		/// The bid price is invalid. It might be lower than or equal to the
		/// current highest bid, or it might be zero.
		InvalidBidPrice,
		/// There are no available auction IDs to be assigned to a new auction.
		NoAvailableAuctionId,
	}

	#[pallet::event]
	#[pallet::generate_deposit(fn deposit_event)]
	pub enum Event<T: Config> {
		/// A bid was successfully placed in an auction.
		Bid {
			/// The ID of the auction.
			auction_id: T::AuctionId,
			/// The account that placed the bid.
			bidder: T::AccountId,
			/// The amount of the bid.
			amount: T::Balance,
		},
	}

	/// Stores ongoing and future auctions. Closed auctions are removed.
	///
	/// Key: Auction ID
	/// Value: Auction information
	#[pallet::storage]
	#[pallet::getter(fn auctions)]
	pub type Auctions<T: Config> = StorageMap<
		_,
		Twox64Concat,
		T::AuctionId,
		AuctionInfo<T::AccountId, T::Balance, BlockNumberFor<T>>,
		OptionQuery,
	>;

	/// Tracks the next available auction ID.
	#[pallet::storage]
	#[pallet::getter(fn auctions_index)]
	pub type AuctionsIndex<T: Config> = StorageValue<_, T::AuctionId, ValueQuery>;

	/// A mapping from block number to a list of auctions that end at that block.
	/// This is used to efficiently process auctions that have ended.
	#[pallet::storage]
	#[pallet::getter(fn auction_end_time)]
	pub type AuctionEndTime<T: Config> =
		StorageDoubleMap<_, Twox64Concat, BlockNumberFor<T>, Blake2_128Concat, T::AuctionId, (), OptionQuery>;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(now: BlockNumberFor<T>) -> Weight {
			T::WeightInfo::on_finalize(AuctionEndTime::<T>::iter_prefix(now).count() as u32)
		}

		fn on_finalize(now: BlockNumberFor<T>) {
			for (auction_id, _) in AuctionEndTime::<T>::drain_prefix(now) {
				if let Some(auction) = Auctions::<T>::take(auction_id) {
					T::Handler::on_auction_ended(auction_id, auction.bid);
				}
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Place a bid in an ongoing auction.
		///
		/// The dispatch origin for this call must be `Signed`.
		///
		/// ## Parameters
		///
		/// - `id`: The ID of the auction to bid on.
		/// - `value`: The amount of the bid.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::bid_collateral_auction())]
		pub fn bid(origin: OriginFor<T>, id: T::AuctionId, #[pallet::compact] value: T::Balance) -> DispatchResult {
			let from = ensure_signed(origin)?;

			Auctions::<T>::try_mutate_exists(id, |auction| -> DispatchResult {
				let auction = auction.as_mut().ok_or(Error::<T>::AuctionNotExist)?;

				let block_number = <frame_system::Pallet<T>>::block_number();

				// make sure auction is started
				ensure!(block_number >= auction.start, Error::<T>::AuctionNotStarted);

				if let Some(ref current_bid) = auction.bid {
					ensure!(value > current_bid.1, Error::<T>::InvalidBidPrice);
				} else {
					ensure!(!value.is_zero(), Error::<T>::InvalidBidPrice);
				}
				let bid_result = T::Handler::on_new_bid(block_number, id, (from.clone(), value), auction.bid.clone());

				ensure!(bid_result.accept_bid, Error::<T>::BidNotAccepted);
				match bid_result.auction_end_change {
					Change::NewValue(new_end) => {
						if let Some(old_end_block) = auction.end {
							AuctionEndTime::<T>::remove(old_end_block, id);
						}
						if let Some(new_end_block) = new_end {
							AuctionEndTime::<T>::insert(new_end_block, id, ());
						}
						auction.end = new_end;
					}
					Change::NoChange => {}
				}
				auction.bid = Some((from.clone(), value));

				Ok(())
			})?;

			Self::deposit_event(Event::Bid {
				auction_id: id,
				bidder: from,
				amount: value,
			});
			Ok(())
		}
	}
}

impl<T: Config> Auction<T::AccountId, BlockNumberFor<T>> for Pallet<T> {
	type AuctionId = T::AuctionId;
	type Balance = T::Balance;

	fn auction_info(id: T::AuctionId) -> Option<AuctionInfo<T::AccountId, T::Balance, BlockNumberFor<T>>> {
		Self::auctions(id)
	}

	fn update_auction(
		id: T::AuctionId,
		info: AuctionInfo<T::AccountId, T::Balance, BlockNumberFor<T>>,
	) -> DispatchResult {
		let auction = Auctions::<T>::get(id).ok_or(Error::<T>::AuctionNotExist)?;
		if let Some(old_end) = auction.end {
			AuctionEndTime::<T>::remove(old_end, id);
		}
		if let Some(new_end) = info.end {
			AuctionEndTime::<T>::insert(new_end, id, ());
		}
		Auctions::<T>::insert(id, info);
		Ok(())
	}

	fn new_auction(
		start: BlockNumberFor<T>,
		end: Option<BlockNumberFor<T>>,
	) -> sp_std::result::Result<T::AuctionId, DispatchError> {
		let auction = AuctionInfo { bid: None, start, end };
		let auction_id =
			<AuctionsIndex<T>>::try_mutate(|n| -> sp_std::result::Result<T::AuctionId, DispatchError> {
				let id = *n;
				*n = n.checked_add(&One::one()).ok_or(Error::<T>::NoAvailableAuctionId)?;
				Ok(id)
			})?;
		Auctions::<T>::insert(auction_id, auction);
		if let Some(end_block) = end {
			AuctionEndTime::<T>::insert(end_block, auction_id, ());
		}

		Ok(auction_id)
	}

	fn remove_auction(id: T::AuctionId) {
		if let Some(auction) = Auctions::<T>::take(id) {
			if let Some(end_block) = auction.end {
				AuctionEndTime::<T>::remove(end_block, id);
			}
		}
	}
}
