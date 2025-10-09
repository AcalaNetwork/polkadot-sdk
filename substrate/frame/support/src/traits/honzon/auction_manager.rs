// This file is part of Substrate.
//
// Copyright (C) 2020-2025 Acala Foundation.
// SPDX-License-Identifier: Apache-2.0
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Auction management traits and supporting types.
//!
//! This module provides abstractions for managing collateral auctions within the Honzon CDP
//! (Collateralized Debt Position) system. The [`AuctionManager`] trait encapsulates the core
//! functionality for creating, canceling, and tracking collateral auctions.
//!
//! # Provided Types
//! - [`AuctionManager`]: Primary interface for managing collateral auctions, including creation,
//!   cancellation, and collateral tracking.

use codec::FullCodec;
use sp_runtime::DispatchResult;
use sp_std::{
	cmp::{Eq, PartialEq},
	fmt::Debug,
};

/// Core interface for managing collateral auctions in the CDP system.
///
/// This trait provides the essential operations for handling collateral auctions, which are
/// used to liquidate undercollateralized positions. Implementers should handle the creation
/// of new auctions, cancellation of existing ones, and tracking of total collateral amounts
/// across all active auctions.
pub trait AuctionManager<AccountId> {
	/// The type of currency used in collateral auctions.
	type CurrencyId;
	/// The type of balance used for collateral amounts and auction targets.
	type Balance;
	/// The type of auction identifier.
	type AuctionId: FullCodec + Debug + Clone + Eq + PartialEq;

	/// Creates a new collateral auction for liquidating undercollateralized positions.
	///
	/// This method initiates a new auction to sell the specified amount of collateral at the
	/// given target price. The refund recipient will receive any proceeds from the auction.
	fn new_collateral_auction(
		refund_recipient: &AccountId,
		currency_id: Self::CurrencyId,
		amount: Self::Balance,
		target: Self::Balance,
	) -> DispatchResult;

	/// Cancels an existing auction and returns any locked collateral.
	///
	/// This method terminates the specified auction and should handle the cleanup of any
	/// associated state and the return of collateral to the appropriate account.
	fn cancel_auction(id: Self::AuctionId) -> DispatchResult;

	/// Returns the total amount of collateral currently in auctions for a specific currency.
	///
	/// This method provides visibility into how much collateral of a given currency type
	/// is currently locked in active auctions.
	fn get_total_collateral_in_auction(currency_id: Self::CurrencyId) -> Self::Balance;

	/// Returns the total target amount across all active auctions.
	///
	/// This method provides the aggregate target amount that all active auctions are
	/// attempting to achieve.
	fn get_total_target_in_auction() -> Self::Balance;
}
