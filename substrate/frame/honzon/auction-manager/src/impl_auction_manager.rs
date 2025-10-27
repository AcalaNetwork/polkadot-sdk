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
	CollateralAuctionItem, CollateralAuctions, Config, Error, Event, HoldReason, Pallet,
	TotalCollateralInAuction, TotalTargetInAuction,
};
use frame_support::{
	pallet_prelude::*,
	traits::{
		fungibles::{Mutate, MutateHold},
		honzon::{Auction, AuctionManager, CDPTreasury, EmergencyShutdown, Rate},
		tokens::{Precision, Preservation},
	},
	transactional,
};
use sp_runtime::{
	traits::{Saturating, Zero},
	FixedPointNumber,
};

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
		// Only native currency is supported as collateral currency.
		ensure!(currency_id == T::GetNativeCurrencyId::get(), Error::<T>::InvalidCurrencyId);
		ensure!(!amount.is_zero(), Error::<T>::InvalidAmount);

		let new_total_collateral = Self::total_collateral_in_auction()
			.checked_add(&amount)
			.ok_or(Error::<T>::InvalidAmount)?;
		let new_total_target = Self::total_target_in_auction()
			.checked_add(&target)
			.ok_or(Error::<T>::InvalidAmount)?;

		let start_time = frame_system::Pallet::<T>::block_number();
		// Add new auction
		let auction_id = pallet_auction::Pallet::<T>::new_auction(start_time, None)?;

		T::Fungibles::transfer(
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
			pallet_auction::Pallet::<T>::auction_info(id).ok_or(Error::<T>::AuctionNotExists)?;

		let collateral_auction =
			Self::collateral_auctions(id).ok_or(Error::<T>::AuctionNotExists)?;

		// If there is a winning bid, process refund of stablecoin to the winning bidder.
		if let Some((bidder, price)) = auction.bid {
			// Compute the bid price as a rate over the initial auction amount.
			let price_rate = Rate::checked_from_rational(price, collateral_auction.initial_amount)
				.unwrap_or_default();
			// Ensure the auction is NOT in its reverse stage which means
			ensure!(!collateral_auction.in_reverse_stage(price_rate), Error::<T>::InReverseStage);

			// Determine payment amount to be refunded to the surplus pool and released from
			// hold.
			let payment_amount = collateral_auction.payment_amount(price_rate);
			// Refund surplus to the CDP treasury. Error if unable.
			T::CDPTreasury::refund_surplus(payment_amount)?;
			// Release the held stablecoin back to the winning bidder. Error if release fails.
			let _ = T::Fungibles::release(
				T::GetStableCurrencyId::get(),
				&HoldReason::CollateralAuction.into(),
				&bidder,
				payment_amount,
				Precision::BestEffort,
			)
			.map_err(|_| Error::<T>::AuctionNotExists)?;
		}

		// Transfer all auctioned collateral back to the refund recipient.
		T::Fungibles::transfer(
			T::GetNativeCurrencyId::get(),
			&T::CDPTreasury::account_id(),
			&collateral_auction.refund_recipient,
			collateral_auction.initial_amount,
			Preservation::Expendable,
		)?;

		// Subtract this auction's initial collateral from the global total in auction.
		TotalCollateralInAuction::<T>::mutate(|total| {
			*total = total.saturating_sub(collateral_auction.initial_amount)
		});
		// Subtract this auction's target value from the total auction target.
		TotalTargetInAuction::<T>::mutate(|total| {
			*total = total.saturating_sub(collateral_auction.target)
		});

		// Remove the collateral auction record.
		CollateralAuctions::<T>::remove(id);
		// Remove the main auction record from the auction pallet.
		pallet_auction::Auctions::<T>::remove(id);

		// Emit an event to notify the auction has been canceled.
		Self::deposit_event(Event::CancelAuction { auction_id: id });

		Ok(())
	}

	fn get_total_collateral_in_auction(currency_id: Self::CurrencyId) -> Self::Balance {
		if currency_id == T::GetNativeCurrencyId::get() {
			TotalCollateralInAuction::<T>::get()
		} else {
			Zero::zero()
		}
	}

	fn get_total_target_in_auction() -> Self::Balance {
		TotalTargetInAuction::<T>::get()
	}
}
