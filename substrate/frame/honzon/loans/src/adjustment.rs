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
	TypeInfo,
	MaxEncodedLen,
)]
pub enum BalanceAdjustment<Balance> {
	/// Increase the balance by this amount.
	Increase(Balance),
	/// Decrease the balance by this amount.
	Decrease(Balance),
}

impl<Balance: Copy> BalanceAdjustment<Balance> {
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
}

impl<Balance: CheckedAdd + CheckedSub> BalanceAdjustment<Balance> {
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
}

impl<Balance: Copy + Saturating> BalanceAdjustment<Balance> {
	/// Apply this adjustment to a current balance using saturating arithmetic.
	pub fn saturating_apply_to(&self, current: Balance) -> Balance {
		match self {
			Self::Increase(amount) => current.saturating_add(*amount),
			Self::Decrease(amount) => current.saturating_sub(*amount),
		}
	}
}

impl<Balance: Zero + Copy> BalanceAdjustment<Balance> {
	pub fn is_zero(&self) -> bool {
		match self {
			Self::Increase(amount) => amount.is_zero(),
			Self::Decrease(amount) => amount.is_zero(),
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
}
