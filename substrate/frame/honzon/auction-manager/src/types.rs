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

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{pallet_prelude::*, traits::honzon::Rate};
use scale_info::TypeInfo;
use sp_runtime::{traits::AtLeast32BitUnsigned, FixedPointNumber};

/// Represents an item up for collateral auction.
#[derive(Encode, Decode, Clone, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[codec(dumb_trait_bound)]
pub struct CollateralAuctionItem<AccountId, BlockNumber, Balance> {
	/// The account to receive a refund if the auction is successful.
	pub refund_recipient: AccountId,
	/// The initial amount of collateral in the auction.
	#[codec(compact)]
	pub initial_amount: Balance,
	/// The current amount of collateral in the auction.
	#[codec(compact)]
	pub amount: Balance,
	/// The target amount to be raised from the auction.
	#[codec(compact)]
	pub target: Balance,
	/// The block number when the auction started.
	pub start_time: BlockNumber,
}

impl<AccountId, BlockNumber, Balance: AtLeast32BitUnsigned + Copy>
	CollateralAuctionItem<AccountId, BlockNumber, Balance>
{
	// No target setting means always forward.
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
		} else if self.in_reverse_stage(bid_price) {
			self.target
		} else {
			bid_price.saturating_mul_int(self.amount)
		}
	}
}
