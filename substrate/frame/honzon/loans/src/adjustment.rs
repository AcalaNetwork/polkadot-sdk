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

//! Balance adjustment types for the loans module.
//!
//! This module provides safe, type-safe representations of balance changes that can be
//! applied to CDP positions. The [`BalanceAdjustment`] type eliminates error-prone
//! conversions and makes the direction of balance changes explicit.

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::traits::{CheckedAdd, CheckedSub, Saturating, Zero};
use sp_std::prelude::*;

/// A safe representation of a balance adjustment that can be either an increase or decrease.
///
/// This type eliminates the error-prone conversions and implicit semantics of the previous
/// `Amount` type by making the direction explicit and using the same underlying type
/// as the balance itself.
#[derive(
	Clone,
	Copy,
	Debug,
	Encode,
	Decode,
	DecodeWithMemTracking,
	Eq,
	PartialEq,
	Ord,
	PartialOrd,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum BalanceAdjustment<Balance> {
	/// Increase the balance by this amount.
	Increase(Balance),
	/// Decrease the balance by this amount.
	Decrease(Balance),
}

impl<Balance> Default for BalanceAdjustment<Balance>
where
	Balance: Zero,
{
	fn default() -> Self {
		Self::Increase(Balance::zero())
	}
}

impl<Balance> BalanceAdjustment<Balance>
where
	Balance: Copy + Zero + PartialOrd + Saturating + CheckedAdd + CheckedSub,
{
	/// Create a new increase adjustment.
	pub fn increase(amount: Balance) -> Self {
		Self::Increase(amount)
	}

	/// Create a new decrease adjustment.
	pub fn decrease(amount: Balance) -> Self {
		Self::Decrease(amount)
	}

	/// Get the absolute amount of this adjustment.
	pub fn amount(&self) -> Balance {
		match self {
			Self::Increase(amount) => *amount,
			Self::Decrease(amount) => *amount,
		}
	}

	/// Check if this is an increase adjustment.
	pub fn is_increase(&self) -> bool {
		matches!(self, Self::Increase(_))
	}

	/// Check if this is a decrease adjustment.
	pub fn is_decrease(&self) -> bool {
		matches!(self, Self::Decrease(_))
	}

	/// Check if this adjustment is zero (no change).
	pub fn is_zero(&self) -> bool {
		self.amount().is_zero()
	}

	/// Apply this adjustment to a current balance, returning the new balance.
	///
	/// Returns `None` if the adjustment would cause underflow.
	pub fn apply_to(&self, current: Balance) -> Result<Balance, sp_arithmetic::ArithmeticError> {
		match self {
			Self::Increase(amount) =>
				current.checked_add(amount).ok_or(sp_arithmetic::ArithmeticError::Overflow),
			Self::Decrease(amount) =>
				current.checked_sub(amount).ok_or(sp_arithmetic::ArithmeticError::Underflow),
		}
	}

	/// Apply this adjustment to a current balance using saturating arithmetic.
	pub fn saturating_apply_to(&self, current: Balance) -> Balance {
		match self {
			Self::Increase(amount) => current.saturating_add(*amount),
			Self::Decrease(amount) => current.saturating_sub(*amount),
		}
	}

	/// Reverse this adjustment (increase becomes decrease and vice versa).
	pub fn reverse(self) -> Self {
		match self {
			Self::Increase(amount) => Self::Decrease(amount),
			Self::Decrease(amount) => Self::Increase(amount),
		}
	}

	/// Combine two adjustments of the same type.
	///
	/// Returns `None` if the adjustments are of different types or would overflow.
	pub fn combine_with(self, other: Self) -> Option<Self> {
		match (&self, &other) {
			(Self::Increase(a), Self::Increase(b)) => a.checked_add(b).map(Self::Increase),
			(Self::Decrease(a), Self::Decrease(b)) => a.checked_add(b).map(Self::Decrease),
			(Self::Increase(a), Self::Decrease(b)) =>
				if a >= b {
					a.checked_sub(b).map(Self::Increase)
				} else {
					b.checked_sub(a).map(Self::Decrease)
				},
			(Self::Decrease(a), Self::Increase(b)) =>
				if b >= a {
					b.checked_sub(a).map(Self::Increase)
				} else {
					a.checked_sub(b).map(Self::Decrease)
				},
		}
	}

	/// Combine two adjustments using saturating arithmetic.
	pub fn saturating_combine_with(self, other: Self) -> Self {
		match (&self, &other) {
			(Self::Increase(a), Self::Increase(b)) => Self::Increase(a.saturating_add(*b)),
			(Self::Decrease(a), Self::Decrease(b)) => Self::Decrease(a.saturating_add(*b)),
			(Self::Increase(a), Self::Decrease(b)) =>
				if a >= b {
					Self::Increase(a.saturating_sub(*b))
				} else {
					Self::Decrease(b.saturating_sub(*a))
				},
			(Self::Decrease(a), Self::Increase(b)) =>
				if b >= a {
					Self::Increase(b.saturating_sub(*a))
				} else {
					Self::Decrease(a.saturating_sub(*b))
				},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_arithmetic::ArithmeticError;

	#[test]
	fn balance_adjustment_basic_operations() {
		let increase = BalanceAdjustment::increase(100u128);
		let decrease = BalanceAdjustment::decrease(50u128);

		assert!(increase.is_increase());
		assert!(!increase.is_decrease());
		assert!(decrease.is_decrease());
		assert!(!decrease.is_increase());

		assert_eq!(increase.amount(), 100u128);
		assert_eq!(decrease.amount(), 50u128);
	}

	#[test]
	fn balance_adjustment_apply_to() {
		let increase = BalanceAdjustment::increase(100u128);
		let decrease = BalanceAdjustment::decrease(50u128);

		assert_eq!(increase.apply_to(200u128), Ok(300u128));
		assert_eq!(decrease.apply_to(200u128), Ok(150u128));
		assert_eq!(decrease.apply_to(30u128), Err(ArithmeticError::Underflow)); // underflow
	}

	#[test]
	fn balance_adjustment_saturating_apply_to() {
		let decrease = BalanceAdjustment::decrease(100u128);
		assert_eq!(decrease.saturating_apply_to(50u128), 0u128); // saturates to 0
	}

	#[test]
	fn balance_adjustment_reverse() {
		let increase = BalanceAdjustment::increase(100u128);
		let reversed = increase.reverse();
		assert!(reversed.is_decrease());
		assert_eq!(reversed.amount(), 100u128);
	}

	#[test]
	fn balance_adjustment_combine() {
		let increase1 = BalanceAdjustment::increase(100u128);
		let increase2 = BalanceAdjustment::increase(50u128);
		let combined = increase1.combine_with(increase2).unwrap();
		assert!(combined.is_increase());
		assert_eq!(combined.amount(), 150u128);

		let decrease = BalanceAdjustment::decrease(75u128);
		let combined = increase1.combine_with(decrease).unwrap();
		assert!(combined.is_increase());
		assert_eq!(combined.amount(), 25u128);

		let decrease2 = BalanceAdjustment::decrease(150u128);
		let combined = increase1.combine_with(decrease2).unwrap();
		assert!(combined.is_decrease());
		assert_eq!(combined.amount(), 50u128);
	}

	#[test]
	fn balance_adjustment_saturating_combine() {
		let increase = BalanceAdjustment::increase(100u128);
		let decrease = BalanceAdjustment::decrease(50u128);
		let combined = increase.saturating_combine_with(decrease);
		assert!(combined.is_increase());
		assert_eq!(combined.amount(), 50u128);
	}
}
