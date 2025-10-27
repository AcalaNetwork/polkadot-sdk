// This file is part of Substrate.
//
// Copyright (C) 2020-2025 Acala Foundation.
// SPDX-License-Identifier: Apache-2.0
//
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

use crate::{
	CollateralAuctions, Config, Event, HoldReason, Pallet, TotalCollateralInAuction,
	TotalTargetInAuction,
};
use frame_support::{
	pallet_prelude::*,
	traits::{
		fungibles::{Mutate, MutateHold},
		honzon::{AuctionHandler, CDPTreasury, Change, OnNewBidResult, Rate},
		tokens::{Precision, Preservation},
	},
};
use frame_system::pallet_prelude::*;
use sp_runtime::{traits::Saturating, FixedPointNumber};

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
		// Retrieve the collateral auction for the given id, failing if not found.
		let mut collateral_auction = match Self::collateral_auctions(id) {
			Some(auction) => auction,
			None => {
				return OnNewBidResult { accept_bid: false, auction_end_change: Change::NoChange };
			},
		};

		let (new_bidder, new_bid_amount) = new_bid;

		// Calculate the price per unit for the new bid.
		// For always_forward auctions, it's the whole price; otherwise, it's per unit.
		let new_bid_price = if collateral_auction.always_forward() {
			Rate::from_rational(new_bid_amount.into(), 1)
		} else {
			Rate::checked_from_rational(new_bid_amount, collateral_auction.initial_amount)
				.unwrap_or_default()
		};

		// For reverse (not always_forward) auctions, enforce the minimum bid logic.
		if !collateral_auction.always_forward() {
			// Compute the target price (what's needed to cover bad debt).
			let target_price = Rate::checked_from_rational(
				collateral_auction.target,
				collateral_auction.initial_amount,
			)
			.unwrap_or_default();

			// Determine the minimum price required for this bid, depending on the last bid.
			let min_price = if let Some((_, last_bid_amount)) = last_bid {
				// Price per unit of last bid.
				let last_price_per_unit =
					Rate::checked_from_rational(last_bid_amount, collateral_auction.initial_amount)
						.unwrap_or_default();
				// If we're in the "reverse stage", keep the price flat;
				// otherwise, require at least the minimum increment.
				if collateral_auction.in_reverse_stage(last_price_per_unit) {
					last_price_per_unit
				} else {
					last_price_per_unit.saturating_add(T::MinimumIncrementSize::get())
				}
			} else {
				// No previous bids: min price is half of target price to stimulate bidding.
				target_price.saturating_mul(Rate::from_rational(1, 2))
			};

			// Reject the bid if it doesn't meet the minimum price requirement.
			if new_bid_price < min_price {
				return OnNewBidResult { accept_bid: false, auction_end_change: Change::NoChange };
			}
		}

		// Calculate the payment amount in stablecoin required from the bidder for this bid.
		let payment_amount = collateral_auction.payment_amount(new_bid_price);

		// If the bid price is greater than the target price, only a portion of the collateral
		// will be sold.
		if collateral_auction.in_reverse_stage(new_bid_price) {
			collateral_auction.amount =
				Rate::checked_from_rational(collateral_auction.target, new_bid_amount)
					.unwrap_or_default()
					.saturating_mul_int(collateral_auction.initial_amount);
			// Persist updated auction state.
			CollateralAuctions::<T>::insert(id, &collateral_auction);
		}

		// Hold the stablecoin payment from the new bidder.
		if T::Fungibles::hold(
			T::GetStableCurrencyId::get(),
			&HoldReason::CollateralAuction.into(),
			&new_bidder,
			payment_amount,
		)
		.is_err()
		{
			// Reject the bid if the payment can't be held (e.g., insufficient funds).
			return OnNewBidResult { accept_bid: false, auction_end_change: Change::NoChange };
		}

		// Forward the held payment to the surplus account.
		T::CDPTreasury::pay_surplus(payment_amount).unwrap_or_default();

		// If there was a previous bid, refund the previous bidder their payment.
		if let Some((last_bidder, last_bid_amount)) = last_bid {
			let last_bid_price =
				Rate::checked_from_rational(last_bid_amount, collateral_auction.initial_amount)
					.unwrap_or_default();
			let last_payment_amount = collateral_auction.payment_amount(last_bid_price);
			let reason: T::RuntimeHoldReason = HoldReason::CollateralAuction.into();
			// Release the previous payment hold.
			let _ = T::Fungibles::release(
				T::GetStableCurrencyId::get(),
				&reason,
				&last_bidder,
				last_payment_amount,
				Precision::BestEffort,
			)
			.ok();
			// Refund surplus account for previous payment.
			T::CDPTreasury::refund_surplus(last_payment_amount).unwrap_or_default();
		}

		// Extend/restart the auction timer based on latest bid time.
		let auction_end_change = Change::NewValue(Some(
			now + Self::get_auction_time_to_close(collateral_auction.start_time, now),
		));

		// Accept the bid and update the auction end time.
		OnNewBidResult { accept_bid: true, auction_end_change }
	}

	fn on_auction_ended(
		id: <T as pallet_auction::Config>::AuctionId,
		winner: Option<(T::AccountId, <T as pallet_auction::Config>::Balance)>,
	) {
		if let Some(collateral_auction) = Self::collateral_auctions(id) {
			TotalCollateralInAuction::<T>::mutate(|total| {
				*total = total.saturating_sub(collateral_auction.initial_amount)
			});
			TotalTargetInAuction::<T>::mutate(|total| {
				*total = total.saturating_sub(collateral_auction.target)
			});

			if let Some((winner, amount)) = winner {
				let price = if collateral_auction.always_forward() {
					Rate::from_rational(amount.into(), 1)
				} else {
					Rate::checked_from_rational(amount, collateral_auction.initial_amount)
						.unwrap_or_default()
				};
				let payment_amount = collateral_auction.payment_amount(price);

				// Release the stablecoin from the winner's hold.
				let _ = T::Fungibles::release(
					T::GetStableCurrencyId::get(),
					&HoldReason::CollateralAuction.into(),
					&winner,
					payment_amount,
					Precision::BestEffort,
				)
				.ok();

				// Transfer the collateral to the winner.
				let _ = T::Fungibles::transfer(
					T::GetNativeCurrencyId::get(),
					&T::CDPTreasury::account_id(),
					&winner,
					collateral_auction.amount,
					Preservation::Expendable,
				);

				// send refund to refund recipient
				if collateral_auction.initial_amount > collateral_auction.amount {
					let _ = T::Fungibles::transfer(
						T::GetNativeCurrencyId::get(),
						&T::CDPTreasury::account_id(),
						&collateral_auction.refund_recipient,
						collateral_auction.initial_amount.saturating_sub(collateral_auction.amount),
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
				// no winner, refund all collateral to refund recipient
				let _ = T::Fungibles::transfer(
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
