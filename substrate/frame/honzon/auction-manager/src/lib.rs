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

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	pallet_prelude::*,
	traits::{
		fungibles::{self, Balanced, Mutate, MutateHold},
		tokens::{Precision, Preservation},
		Get,
	},
	transactional,
};
use frame_system::pallet_prelude::*;
use pallet_auction;
use pallet_traits::{
	Auction, AuctionHandler, AuctionInfo, AuctionManager, CDPTreasury, Change, EmergencyShutdown,
	OnNewBidResult, PriceProvider, Rate, Swap,
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{AtLeast32BitUnsigned, CheckedDiv, CheckedMul, One, Saturating, Zero},
	DispatchError, DispatchResult, FixedPointNumber, RuntimeDebug,
};

mod mock;
mod tests;
pub mod weights;

pub use weights::WeightInfo;

/// Reasons for holding funds in this pallet.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[codec(dumb_trait_bound)]
pub enum HoldReason {
	/// Funds are held for a collateral auction.
	CollateralAuction,
}

/// Represents an item up for collateral auction.
#[derive(Encode, Decode, Clone, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[codec(dumb_trait_bound)]
pub struct CollateralAuctionItem<AccountId, BlockNumber, Balance> {
	/// The account to receive a refund if the auction is successful.
	refund_recipient: AccountId,
	/// The initial amount of collateral in the auction.
	#[codec(compact)]
	initial_amount: Balance,
	/// The current amount of collateral in the auction.
	#[codec(compact)]
	amount: Balance,
	/// The target amount to be raised from the auction.
	#[codec(compact)]
	target: Balance,
	/// The block number when the auction started.
	start_time: BlockNumber,
}

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
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		/// The currency type for interacting with this pallet.
		type CurrencyId: frame_support::pallet_prelude::Parameter
			+ sp_std::cmp::PartialEq
			+ sp_std::cmp::Eq
			+ sp_std::marker::Copy
			+ sp_std::cmp::Ord
			+ sp_std::marker::Send
			+ sp_std::marker::Sync
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
		/// The swap pallet.
		type Swap: Swap<
			Self::AccountId,
			<Self as pallet_auction::Config>::Balance,
			Self::CurrencyId,
		>;
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
		type Currency: fungibles::Mutate<
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
		/// The auction pallet.
		type Auction: Auction<Self::AccountId, BlockNumberFor<Self>>;
		/// The emergency shutdown pallet.
		type EmergencyShutdown: EmergencyShutdown;
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
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
	pub type TotalCollateralInAuction<T: Config> =
		StorageValue<_, <T as pallet_auction::Config>::Balance, ValueQuery>;

	/// The total target amount to be raised from all auctions.
	#[pallet::storage]
	pub type TotalTargetInAuction<T: Config> =
		StorageValue<_, <T as pallet_auction::Config>::Balance, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {}

	impl<AccountId, BlockNumber, Balance: AtLeast32BitUnsigned + Copy>
		CollateralAuctionItem<AccountId, BlockNumber, Balance>
	{
		pub fn always_forward(&self) -> bool {
			self.target.is_zero()
		}

		// true if in reverse stage, price is per unit of collateral
		pub fn in_reverse_stage(&self, bid_price: Rate) -> bool {
			if self.always_forward() {
				return false;
			}
			// won't overflow since initial_amount is not zero.
			let target_price =
				Rate::checked_from_rational(self.target, self.initial_amount).unwrap_or_default();
			bid_price >= target_price
		}

		// stable coin amount to pay
		pub fn payment_amount(&self, bid_price: Rate) -> Balance {
			if self.always_forward() {
				bid_price.saturating_mul_int(self.amount)
			} else {
				if self.in_reverse_stage(bid_price) {
					self.target
				} else {
					bid_price.saturating_mul_int(self.amount)
				}
			}
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn get_auction_time_to_close(
			auction_start_time: BlockNumberFor<T>,
			now: BlockNumberFor<T>,
		) -> BlockNumberFor<T> {
			let auction_duration_soft_cap = T::AuctionDurationSoftCap::get();
			let time_to_close = T::AuctionTimeToClose::get();
			if now >= auction_start_time + auction_duration_soft_cap {
				time_to_close / 2u32.into()
			} else {
				time_to_close
			}
		}
	}

	impl<T: Config> AuctionManager<T::AccountId> for Pallet<T> {
		type CurrencyId = T::CurrencyId;
		type Balance = <T as pallet_auction::Config>::Balance;
		type AuctionId = <T as pallet_auction::Config>::AuctionId;

		#[transactional]
		fn new_collateral_auction(
			refund_recipient: &T::AccountId,
			currency_id: Self::CurrencyId,
			amount: Self::Balance,
			target: Self::Balance,
		) -> DispatchResult {
			ensure!(currency_id == T::GetNativeCurrencyId::get(), Error::<T>::InvalidCurrencyId);
			ensure!(!amount.is_zero(), Error::<T>::InvalidAmount);

			let new_total_collateral = TotalCollateralInAuction::<T>::get()
				.checked_add(&amount)
				.ok_or(Error::<T>::InvalidAmount)?;
			let new_total_target = TotalTargetInAuction::<T>::get()
				.checked_add(&target)
				.ok_or(Error::<T>::InvalidAmount)?;

			let start_time = frame_system::Pallet::<T>::block_number();
			let auction_id = pallet_auction::Pallet::<T>::new_auction(start_time, None)?;

			T::Currency::transfer(
				currency_id,
				refund_recipient,
				&T::CDPTreasury::account_id(),
				amount,
				Preservation::Expendable,
			)?;

			let collateral_auction = CollateralAuctionItem {
				refund_recipient: refund_recipient.clone(),
				initial_amount: amount,
				amount,
				target,
				start_time,
			};
			CollateralAuctions::<T>::insert(auction_id, collateral_auction);

			TotalCollateralInAuction::<T>::put(new_total_collateral);
			TotalTargetInAuction::<T>::put(new_total_target);

			Self::deposit_event(Event::NewCollateralAuction {
				auction_id,
				collateral_type: currency_id,
				collateral_amount: amount,
				target_bid_price: target,
			});

			Ok(())
		}

		#[transactional]
		fn cancel_auction(id: Self::AuctionId) -> DispatchResult {
			ensure!(T::EmergencyShutdown::is_shutdown(), Error::<T>::MustAfterShutdown);

			let auction =
				pallet_auction::Auctions::<T>::get(id).ok_or(Error::<T>::AuctionNotExists)?;
			let collateral_auction =
				CollateralAuctions::<T>::get(id).ok_or(Error::<T>::AuctionNotExists)?;

			if let Some((bidder, price)) = auction.bid {
				let price_rate =
					Rate::checked_from_rational(price, collateral_auction.initial_amount)
						.unwrap_or_default();
				ensure!(
					!collateral_auction.in_reverse_stage(price_rate),
					Error::<T>::InReverseStage
				);

				let payment_amount = collateral_auction.payment_amount(price_rate);
				T::CDPTreasury::refund_surplus(payment_amount)?;
				let reason: T::RuntimeHoldReason = HoldReason::CollateralAuction.into();
				let _ = T::Currency::release(
					T::GetStableCurrencyId::get(),
					&reason,
					&bidder,
					payment_amount,
					Precision::BestEffort,
				)
				.map_err(|_| Error::<T>::AuctionNotExists)?;
			}

			T::Currency::transfer(
				T::GetNativeCurrencyId::get(),
				&T::CDPTreasury::account_id(),
				&collateral_auction.refund_recipient,
				collateral_auction.initial_amount,
				Preservation::Expendable,
			)?;

			TotalCollateralInAuction::<T>::mutate(|total| {
				*total = total.saturating_sub(collateral_auction.initial_amount)
			});
			TotalTargetInAuction::<T>::mutate(|total| {
				*total = total.saturating_sub(collateral_auction.target)
			});

			CollateralAuctions::<T>::remove(id);
			pallet_auction::Auctions::<T>::remove(id);

			Self::deposit_event(Event::CancelAuction { auction_id: id });

			Ok(())
		}

		fn get_total_collateral_in_auction(id: Self::CurrencyId) -> Self::Balance {
			if id == T::GetNativeCurrencyId::get() {
				TotalCollateralInAuction::<T>::get()
			} else {
				Zero::zero()
			}
		}

		fn get_total_target_in_auction() -> Self::Balance {
			TotalTargetInAuction::<T>::get()
		}
	}

	impl<T: Config>
		AuctionHandler<
			T::AccountId,
			<T as pallet_auction::Config>::Balance,
			BlockNumberFor<T>,
			<T as pallet_auction::Config>::AuctionId,
		> for Pallet<T>
	where
		<T as pallet_auction::Config>::Balance: Into<u128>,
	{
		fn on_new_bid(
			now: BlockNumberFor<T>,
			id: <T as pallet_auction::Config>::AuctionId,
			new_bid: (T::AccountId, <T as pallet_auction::Config>::Balance),
			last_bid: Option<(T::AccountId, <T as pallet_auction::Config>::Balance)>,
		) -> OnNewBidResult<BlockNumberFor<T>> {
			let mut collateral_auction = if let Some(auction) = CollateralAuctions::<T>::get(id) {
				auction
			} else {
				return OnNewBidResult { accept_bid: false, auction_end_change: Change::NoChange };
			};

			let (new_bidder, new_bid_price) = new_bid;

			let new_price_per_unit = if collateral_auction.always_forward() {
				Rate::from_rational(new_bid_price.into(), 1)
			} else {
				Rate::checked_from_rational(new_bid_price, collateral_auction.initial_amount)
					.unwrap_or_default()
			};

			if !collateral_auction.always_forward() {
				let target_price = Rate::checked_from_rational(
					collateral_auction.target,
					collateral_auction.initial_amount,
				)
				.unwrap_or_default();
				let min_price = if let Some((_, last_bid_price)) = last_bid {
					let last_price_per_unit = Rate::checked_from_rational(
						last_bid_price,
						collateral_auction.initial_amount,
					)
					.unwrap_or_default();
					if collateral_auction.in_reverse_stage(last_price_per_unit) {
						last_price_per_unit
					} else {
						last_price_per_unit.saturating_add(T::MinimumIncrementSize::get())
					}
				} else {
					target_price.saturating_mul(Rate::from_rational(1, 2))
				};

				if new_price_per_unit < min_price {
					return OnNewBidResult {
						accept_bid: false,
						auction_end_change: Change::NoChange,
					};
				}
			}

			let payment_amount = collateral_auction.payment_amount(new_price_per_unit);
			if collateral_auction.in_reverse_stage(new_price_per_unit) {
				collateral_auction.amount =
					Rate::checked_from_rational(collateral_auction.target, new_bid_price)
						.unwrap_or_default()
						.saturating_mul_int(collateral_auction.initial_amount);
				CollateralAuctions::<T>::insert(id, &collateral_auction);
			}

			let reason: T::RuntimeHoldReason = HoldReason::CollateralAuction.into();
			if T::Currency::hold(
				T::GetStableCurrencyId::get(),
				&reason,
				&new_bidder,
				payment_amount,
			)
			.is_err()
			{
				return OnNewBidResult { accept_bid: false, auction_end_change: Change::NoChange };
			}
			T::CDPTreasury::pay_surplus(payment_amount).unwrap_or_default();

			if let Some((last_bidder, last_bid_price)) = last_bid {
				let last_price_per_unit =
					Rate::checked_from_rational(last_bid_price, collateral_auction.initial_amount)
						.unwrap_or_default();
				let last_payment_amount = collateral_auction.payment_amount(last_price_per_unit);
				let reason: T::RuntimeHoldReason = HoldReason::CollateralAuction.into();
				let _ = T::Currency::release(
					T::GetStableCurrencyId::get(),
					&reason,
					&last_bidder,
					last_payment_amount,
					Precision::BestEffort,
				)
				.ok();
				T::CDPTreasury::refund_surplus(last_payment_amount).unwrap_or_default();
			}

			let auction_end_change = Change::NewValue(Some(
				now + Self::get_auction_time_to_close(collateral_auction.start_time, now),
			));

			OnNewBidResult { accept_bid: true, auction_end_change }
		}

		fn on_auction_ended(
			id: <T as pallet_auction::Config>::AuctionId,
			winner: Option<(T::AccountId, <T as pallet_auction::Config>::Balance)>,
		) {
			if let Some(collateral_auction) = CollateralAuctions::<T>::get(id) {
				TotalCollateralInAuction::<T>::mutate(|total| {
					*total = total.saturating_sub(collateral_auction.initial_amount)
				});
				TotalTargetInAuction::<T>::mutate(|total| {
					*total = total.saturating_sub(collateral_auction.target)
				});

				if let Some((winner, price)) = winner {
					let price_per_unit = if collateral_auction.always_forward() {
						Rate::from_rational(price.into(), 1)
					} else {
						Rate::checked_from_rational(price, collateral_auction.initial_amount)
							.unwrap_or_default()
					};
					let payment_amount = collateral_auction.payment_amount(price_per_unit);

					let reason: T::RuntimeHoldReason = HoldReason::CollateralAuction.into();
					let _ = T::Currency::release(
						T::GetStableCurrencyId::get(),
						&reason,
						&winner,
						payment_amount,
						Precision::BestEffort,
					)
					.ok();
					let _ = T::Currency::transfer(
						T::GetNativeCurrencyId::get(),
						&T::CDPTreasury::account_id(),
						&winner,
						collateral_auction.amount,
						Preservation::Expendable,
					);

					// send refund to refund_recipient
					if collateral_auction.initial_amount > collateral_auction.amount {
						let _ = T::Currency::transfer(
							T::GetNativeCurrencyId::get(),
							&T::CDPTreasury::account_id(),
							&collateral_auction.refund_recipient,
							collateral_auction
								.initial_amount
								.saturating_sub(collateral_auction.amount),
							Preservation::Expendable,
						);
					}

					Self::deposit_event(Event::CollateralAuctionDealt {
						auction_id: id,
						collateral_type: T::GetNativeCurrencyId::get(),
						collateral_amount: collateral_auction.amount,
						winner,
						payment_amount,
					});
				} else {
					// no winner, abort
					let _ = T::Currency::transfer(
						T::GetNativeCurrencyId::get(),
						&T::CDPTreasury::account_id(),
						&collateral_auction.refund_recipient,
						collateral_auction.initial_amount,
						Preservation::Expendable,
					);

					Self::deposit_event(Event::CollateralAuctionAborted {
						auction_id: id,
						collateral_type: T::GetNativeCurrencyId::get(),
						collateral_amount: collateral_auction.initial_amount,
						target_stable_amount: collateral_auction.target,
						refund_recipient: collateral_auction.refund_recipient,
					});
				}

				CollateralAuctions::<T>::remove(id);
			}
		}
	}
}
