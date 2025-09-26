//! This module provides traits and types for a generic auction system.

use crate::Change;
use codec::{Decode, Encode, FullCodec, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{AtLeast32Bit, Bounded, MaybeSerializeDeserialize},
	DispatchError, DispatchResult, RuntimeDebug,
};
use sp_std::{
	cmp::{Eq, PartialEq},
	fmt::Debug,
	result,
};

/// Represents the state of an auction.
#[cfg_attr(feature = "std", derive(PartialEq, Eq))]
#[derive(Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct AuctionInfo<AccountId, Balance, BlockNumber> {
	/// The current bidder and their bid, if any.
	pub bid: Option<(AccountId, Balance)>,
	/// The block number at which the auction started.
	pub start: BlockNumber,
	/// The block number at which the auction will end, if set.
	pub end: Option<BlockNumber>,
}

/// A trait for managing auctions.
pub trait Auction<AccountId, BlockNumber> {
	/// The type used to identify an auction.
	type AuctionId: FullCodec
		+ Default
		+ Copy
		+ Eq
		+ PartialEq
		+ MaybeSerializeDeserialize
		+ Bounded
		+ Debug;
	/// The type used to represent the bid price.
	type Balance: AtLeast32Bit + FullCodec + Copy + MaybeSerializeDeserialize + Debug + Default;

	/// Returns the information for a given auction.
	fn auction_info(
		id: Self::AuctionId,
	) -> Option<AuctionInfo<AccountId, Self::Balance, BlockNumber>>;
	/// Updates the information for a given auction.
	fn update_auction(
		id: Self::AuctionId,
		info: AuctionInfo<AccountId, Self::Balance, BlockNumber>,
	) -> DispatchResult;
	/// Creates a new auction.
	///
	/// Returns the ID of the new auction.
	fn new_auction(
		start: BlockNumber,
		end: Option<BlockNumber>,
	) -> result::Result<Self::AuctionId, DispatchError>;
	/// Removes an auction.
	fn remove_auction(id: Self::AuctionId);
}

/// The result of handling a new bid.
pub struct OnNewBidResult<BlockNumber> {
	/// Whether the bid was accepted.
	pub accept_bid: bool,
	/// A potential change to the auction's end time.
	pub auction_end_change: Change<Option<BlockNumber>>,
}

/// A trait for handling auction events.
pub trait AuctionHandler<AccountId, Balance, BlockNumber, AuctionId> {
	/// Called when a new bid is received.
	///
	/// The return value determines whether the bid should be accepted and whether
	/// the auction's end time should be updated. The implementation should
	/// reserve funds from the new bidder and refund the previous bidder.
	fn on_new_bid(
		now: BlockNumber,
		id: AuctionId,
		new_bid: (AccountId, Balance),
		last_bid: Option<(AccountId, Balance)>,
	) -> OnNewBidResult<BlockNumber>;
	/// Called when an auction has ended.
	fn on_auction_ended(id: AuctionId, winner: Option<(AccountId, Balance)>);
}
