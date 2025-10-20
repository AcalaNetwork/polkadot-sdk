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

//! # CDP Treasury Traits Implementations
use crate::*;
use frame_support::{
	traits::{
		fungibles::{Inspect, Mutate},
		honzon::{AuctionManager, CDPTreasury, CDPTreasuryExtended, Ratio, SwapLimit},
		tokens::{Fortitude, Precision, Preservation},
	},
	transactional,
};
use pallet_asset_conversion::Swap;
use sp_runtime::{
	traits::{One, Saturating, Zero},
	ArithmeticError, DispatchError, DispatchResult, FixedPointNumber,
};

impl<T: Config> CDPTreasury<T::AccountId> for Pallet<T> {
	type Balance = T::Balance;
	type CurrencyId = T::CurrencyId;

	/// The account ID of the cdp-treasury pallet.
	fn account_id() -> T::AccountId {
		Self::account_id()
	}

	/// Pays a surplus.
	fn pay_surplus(amount: Self::Balance) -> DispatchResult {
		T::Fungibles::mint_into(T::StableCurrencyId::get(), &Self::account_id(), amount).map(|_| ())
	}

	/// Refunds a surplus.
	fn refund_surplus(amount: Self::Balance) -> DispatchResult {
		T::Fungibles::burn_from(
			T::StableCurrencyId::get(),
			&Self::account_id(),
			amount,
			Preservation::Expendable,
			Precision::Exact,
			Fortitude::Polite,
		)
		.map(|_| ())
	}

	/// Gets the surplus pool.
	fn get_surplus_pool() -> Self::Balance {
		Self::surplus_pool()
	}

	/// Gets the debit pool.
	fn get_debit_pool() -> Self::Balance {
		Self::debit_pool()
	}

	/// Gets the total collaterals.
	fn get_total_collaterals() -> Self::Balance {
		Self::total_collaterals()
	}

	/// Deposits collateral.
	fn deposit_collateral(_from: &T::AccountId, _amount: Self::Balance) -> DispatchResult {
		T::Fungibles::transfer(
			T::CollateralCurrencyId::get(),
			_from,
			&Self::account_id(),
			_amount,
			Preservation::Expendable,
		)
		.map(|_| ())
	}

	/// Withdraws collateral.
	fn withdraw_collateral(_to: &T::AccountId, _amount: Self::Balance) -> DispatchResult {
		T::Fungibles::transfer(
			T::CollateralCurrencyId::get(),
			&Self::account_id(),
			_to,
			_amount,
			Preservation::Expendable,
		)
		.map(|_| ())
	}

	/// Gets the debit proportion.
	fn get_debit_proportion(amount: Self::Balance) -> Ratio {
		let stable_total_supply = T::Fungibles::total_issuance(T::StableCurrencyId::get());
		Ratio::checked_from_rational(amount, stable_total_supply).unwrap_or_default()
	}

	/// Handles a system debit.
	fn on_system_debit(amount: Self::Balance) -> DispatchResult {
		DebitPool::<T>::try_mutate(|debit_pool| -> DispatchResult {
			*debit_pool = debit_pool.checked_add(&amount).ok_or(ArithmeticError::Overflow)?;
			Ok(())
		})
	}

	/// Handles a system surplus.
	fn on_system_surplus(amount: Self::Balance) -> DispatchResult {
		Self::issue_debit(&Self::account_id(), amount, true)
	}

	/// Issues a debit. This should be the only function in the system that issues stable coin.
	fn issue_debit(who: &T::AccountId, debit: Self::Balance, backed: bool) -> DispatchResult {
		// increase system debit if the debit is unbacked
		if !backed {
			Self::on_system_debit(debit)?;
		}
		T::Fungibles::mint_into(T::StableCurrencyId::get(), who, debit)?;

		Ok(())
	}

	/// Burns a debit. This should be the only function in the system that burns stable coin.
	fn burn_debit(who: &T::AccountId, debit: Self::Balance) -> DispatchResult {
		T::Fungibles::burn_from(
			T::StableCurrencyId::get(),
			who,
			debit,
			Preservation::Expendable,
			Precision::Exact,
			Fortitude::Polite,
		)
		.map(|_| ())
	}

	/// Deposits a surplus.
	fn deposit_surplus(from: &T::AccountId, surplus: Self::Balance) -> DispatchResult {
		T::Fungibles::transfer(
			T::StableCurrencyId::get(),
			from,
			&Self::account_id(),
			surplus,
			Preservation::Preserve,
		)
		.map(|_| ())
	}

	/// Withdraws a surplus.
	fn withdraw_surplus(to: &T::AccountId, surplus: Self::Balance) -> DispatchResult {
		T::Fungibles::transfer(
			T::StableCurrencyId::get(),
			&Self::account_id(),
			to,
			surplus,
			Preservation::Expendable,
		)
		.map(|_| ())
	}
}

impl<T: Config> CDPTreasuryExtended<T::AccountId> for Pallet<T> {
	/// Swaps collateral to stable currency.
	#[transactional]
	fn swap_collateral_to_stable(
		limit: SwapLimit<T::Balance>,
		collateral_in_auction: bool,
	) -> sp_std::result::Result<(T::Balance, T::Balance), DispatchError> {
		let supply_limit = match limit {
			SwapLimit::ExactSupply(supply_amount, _) => supply_amount,
			SwapLimit::ExactTarget(max_supply_amount, _) => max_supply_amount,
		};

		if collateral_in_auction {
			ensure!(
				Self::total_collaterals() >= supply_limit &&
					T::AuctionManagerHandler::get_total_collateral_in_auction(
						T::CollateralCurrencyId::get()
					) >= supply_limit,
				Error::<T>::CollateralNotEnough,
			);
		} else {
			ensure!(
				Self::total_collaterals_not_in_auction() >= supply_limit,
				Error::<T>::CollateralNotEnough,
			);
		}

		let path = vec![T::CollateralCurrencyId::get().into(), T::StableCurrencyId::get().into()];
		match limit {
			SwapLimit::ExactSupply(supply_amount, min_target_amount) => {
				let swapped = T::Swap::swap_exact_tokens_for_tokens(
					Self::account_id(),
					path,
					supply_amount,
					Some(min_target_amount),
					Self::account_id(),
					true,
				)?;
				Ok((supply_amount, swapped))
			},
			SwapLimit::ExactTarget(max_supply_amount, exact_target_amount) => {
				let swapped = T::Swap::swap_tokens_for_exact_tokens(
					Self::account_id(),
					path,
					exact_target_amount,
					Some(max_supply_amount),
					Self::account_id(),
					true,
				)?;
				Ok((swapped, exact_target_amount))
			},
		}
	}

	/// Creates collateral auctions.
	fn create_collateral_auctions(
		amount: T::Balance,
		target: T::Balance,
		refund_receiver: T::AccountId,
		split: bool,
	) -> Result<u32, DispatchError> {
		ensure!(
			Self::total_collaterals_not_in_auction() >= amount,
			Error::<T>::CollateralNotEnough,
		);

		let mut unhandled_collateral_amount = amount;
		let mut unhandled_target = target;
		let expected_collateral_auction_size = Self::expected_collateral_auction_size();
		let max_auctions_count: T::Balance = T::MaxAuctionsCount::get().into();

		// Determine the number of lots to create based on splitting configuration
		let lots_count = if !split ||
			max_auctions_count.is_zero() ||
			expected_collateral_auction_size.is_zero() ||
			amount <= expected_collateral_auction_size
		{
			One::one()
		} else {
			let mut count = amount
				.checked_div(&expected_collateral_auction_size)
				.expect("collateral auction maximum size is not zero; qed");

			let remainder = amount
				.checked_rem(&expected_collateral_auction_size)
				.expect("collateral auction maximum size is not zero; qed");
			if !remainder.is_zero() {
				count = count.saturating_add(One::one());
			}
			sp_std::cmp::min(count, max_auctions_count)
		};

		// Pre-calculate average amounts per lot for even distribution
		let average_amount_per_lot =
			amount.checked_div(&lots_count).expect("lots count is at least 1; qed");
		let average_target_per_lot =
			target.checked_div(&lots_count).expect("lots count is at least 1; qed");
		let mut created_lots: T::Balance = Zero::zero();

		// Create auction lots iteratively
		while !unhandled_collateral_amount.is_zero() {
			created_lots = created_lots.saturating_add(One::one());

			// Determine lot size: last lot gets remaining amounts, others get average
			let (lot_collateral_amount, lot_target) = if created_lots == lots_count {
				// Last lot: use all remaining amounts (handles any remainder)
				(unhandled_collateral_amount, unhandled_target)
			} else {
				// Regular lot: use pre-calculated average amounts
				(average_amount_per_lot, average_target_per_lot)
			};

			// Create the auction lot
			T::AuctionManagerHandler::new_collateral_auction(
				&refund_receiver,
				T::CollateralCurrencyId::get(),
				lot_collateral_amount,
				lot_target,
			)?;

			// Update remaining amounts
			unhandled_collateral_amount =
				unhandled_collateral_amount.saturating_sub(lot_collateral_amount);
			unhandled_target = unhandled_target.saturating_sub(lot_target);
		}

		let created_auctions: u32 =
			created_lots.try_into().map_err(|_| ArithmeticError::Overflow)?;
		Ok(created_auctions)
	}

	/// The maximum number of auctions.
	fn max_auction() -> u32 {
		T::MaxAuctionsCount::get()
	}
}
