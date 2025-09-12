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

//! This module provides the core traits for the Honzon protocol.

use codec::{Decode, Encode, FullCodec, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::U256;
use sp_runtime::{DispatchError, DispatchResult, RuntimeDebug};
use sp_std::{
	cmp::{Eq, PartialEq},
	fmt::Debug,
	prelude::*,
};

use crate::{dex::*, ExchangeRate, Ratio};

/// A collateralized debt position.
#[derive(
	Encode, Decode, Eq, PartialEq, Copy, Clone, RuntimeDebug, Default, MaxEncodedLen, TypeInfo,
)]
pub struct Position<Balance> {
	/// The amount of collateral.
	pub collateral: Balance,
	/// The amount of debit.
	pub debit: Balance,
}

/// A trait for managing the risk of the protocol.
pub trait RiskManager<AccountId, CurrencyId, Balance, DebitBalance> {
	/// Returns the value of a given amount of debit.
	fn get_debit_value(currency_id: CurrencyId, debit_balance: DebitBalance) -> Balance;

	/// Checks if a position is valid.
	fn check_position_valid(
		currency_id: CurrencyId,
		collateral_balance: Balance,
		debit_balance: DebitBalance,
		check_required_ratio: bool,
	) -> DispatchResult;

	/// Checks if the total debit for a currency has reached its cap.
	fn check_debit_cap(
		currency_id: CurrencyId,
		total_debit_balance: DebitBalance,
	) -> DispatchResult;
}

#[cfg(feature = "std")]
impl<AccountId, CurrencyId, Balance: Default, DebitBalance>
	RiskManager<AccountId, CurrencyId, Balance, DebitBalance> for ()
{
	fn get_debit_value(_currency_id: CurrencyId, _debit_balance: DebitBalance) -> Balance {
		Default::default()
	}

	fn check_position_valid(
		_currency_id: CurrencyId,
		_collateral_balance: Balance,
		_debit_balance: DebitBalance,
		_check_required_ratio: bool,
	) -> DispatchResult {
		Ok(())
	}

	fn check_debit_cap(
		_currency_id: CurrencyId,
		_total_debit_balance: DebitBalance,
	) -> DispatchResult {
		Ok(())
	}
}

/// A trait for managing auctions.
pub trait AuctionManager<AccountId> {
	/// The type of currency used in auctions.
	type CurrencyId;
	/// The type of balance used in auctions.
	type Balance;
	/// The type of auction ID.
	type AuctionId: FullCodec + Debug + Clone + Eq + PartialEq;

	/// Creates a new collateral auction.
	fn new_collateral_auction(
		refund_recipient: &AccountId,
		currency_id: Self::CurrencyId,
		amount: Self::Balance,
		target: Self::Balance,
	) -> DispatchResult;
	/// Cancels an auction.
	fn cancel_auction(id: Self::AuctionId) -> DispatchResult;
	/// Returns the total amount of collateral in auctions for a given currency.
	fn get_total_collateral_in_auction(currency_id: Self::CurrencyId) -> Self::Balance;
	/// Returns the total target amount in auctions.
	fn get_total_target_in_auction() -> Self::Balance;
}

/// A trait for managing the Collateralized Debt Position (CDP) treasury.
pub trait CDPTreasury<AccountId> {
	/// Returns the account ID of the treasury.
	fn account_id() -> AccountId;
	/// The type of balance used in the treasury.
	type Balance;
	/// The type of currency used in the treasury.
	type CurrencyId;

	/// Returns the amount of surplus in the treasury.
	fn get_surplus_pool() -> Self::Balance;

	/// Returns the amount of debit in the treasury.
	fn get_debit_pool() -> Self::Balance;

	/// Returns the total amount of collateral in the treasury.
	fn get_total_collaterals() -> Self::Balance;

	/// Calculates the proportion of a specific debit amount relative to the whole system.
	fn get_debit_proportion(amount: Self::Balance) -> Ratio;

	/// Handles a system-wide debit event.
	fn on_system_debit(amount: Self::Balance) -> DispatchResult;

	/// Handles a system-wide surplus event.
	fn on_system_surplus(amount: Self::Balance) -> DispatchResult;

	/// Issues debit to a specified account.
	///
	/// If `backed` is true, the debit is backed by some assets; otherwise, the system
	/// debit will be increased by the same amount.
	fn issue_debit(who: &AccountId, debit: Self::Balance, backed: bool) -> DispatchResult;

	/// Burns debit from a specified account.
	fn burn_debit(who: &AccountId, debit: Self::Balance) -> DispatchResult;

	/// Deposits surplus from a specified account into the treasury.
	fn deposit_surplus(from: &AccountId, surplus: Self::Balance) -> DispatchResult;

	/// Withdraws surplus from the treasury to a specified account.
	fn withdraw_surplus(to: &AccountId, surplus: Self::Balance) -> DispatchResult;

	/// Deposits collateral from a specified account into the treasury.
	fn deposit_collateral(from: &AccountId, amount: Self::Balance) -> DispatchResult;

	/// Withdraws collateral from the treasury to a specified account.
	fn withdraw_collateral(to: &AccountId, amount: Self::Balance) -> DispatchResult;

	/// Pays surplus from the treasury.
	fn pay_surplus(amount: Self::Balance) -> DispatchResult;

	/// Refunds surplus to the treasury.
	fn refund_surplus(amount: Self::Balance) -> DispatchResult;
}

/// An extended `CDPTreasury` trait.
pub trait CDPTreasuryExtended<AccountId>: CDPTreasury<AccountId> {
	/// Swaps collateral to the stable currency.
	fn swap_collateral_to_stable(
		limit: SwapLimit<Self::Balance>,
		collateral_in_auction: bool,
	) -> sp_std::result::Result<(Self::Balance, Self::Balance), DispatchError>;

	/// Creates collateral auctions.
	fn create_collateral_auctions(
		amount: Self::Balance,
		target: Self::Balance,
		refund_receiver: AccountId,
		split: bool,
	) -> sp_std::result::Result<u32, DispatchError>;

	/// Returns the maximum number of auctions that can be created.
	fn max_auction() -> u32;
}

/// A trait for handling emergency shutdowns.
pub trait EmergencyShutdown {
	/// Returns `true` if the system is in shutdown mode.
	fn is_shutdown() -> bool;
}

/// A trait for managing the Honzon protocol, intended for use with EVM+.
pub trait HonzonManager<AccountId, Amount, Balance> {
	/// Adjusts a CDP loan.
	fn adjust_loan(
		who: &AccountId,
		collateral_adjustment: Amount,
		debit_adjustment: Amount,
	) -> DispatchResult;
	/// Closes a CDP loan using a DEX.
	fn close_loan_by_dex(who: AccountId, max_collateral_amount: Balance) -> DispatchResult;
	/// Returns the CDP for a given account.
	fn get_position(who: &AccountId) -> Position<Balance>;
	/// Returns the liquidation ratio for the collateral.
	fn get_collateral_parameters() -> Vec<U256>;
	/// Returns the current collateral-to-debit ratio of a CDP.
	fn get_current_collateral_ratio(who: &AccountId) -> Option<Ratio>;
	/// Returns the exchange rate of debit units to debit value.
	fn get_debit_exchange_rate() -> ExchangeRate;
}
