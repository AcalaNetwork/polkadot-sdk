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

//! # CDP Engine
//!
//! ## Overview
//!
//! The CDP Engine pallet is the core component of the Honzon protocol, responsible for managing
//! Collateralized Debt Positions (CDPs). It handles the internal processes of CDPs, including
//! liquidation, settlement, and risk management. This pallet works in conjunction with
//! `pallet-loans` to manage the collateral and debt of each position, `pallet-dex` for
//! liquidating collateral, and an oracle for price feeds.
//!
//! ### Key Concepts
//!
//! * **Collateralized Debt Position (CDP):** A CDP is a loan where a user locks up collateral
//!   (e.g., DOT) to borrow a stablecoin (e.g., aUSD).
//! * **Liquidation:** If the value of the collateral drops and the collateral-to-debt ratio falls
//!   below a certain threshold (the liquidation ratio), the CDP is considered unsafe and can be
//!   liquidated. During liquidation, the collateral is sold to cover the debt, plus a penalty.
//! * **Settlement:** In the case of a global shutdown of the system, this pallet handles the
//!   settlement of all outstanding CDPs.
//! * **Risk Management:** The pallet includes several parameters to manage the risk of the system,
//!   such as liquidation ratios, liquidation penalties, and debt ceilings for each collateral type.
//!
//! ## Interface
//!
//! ### Dispatchable Functions
//!
//! * `liquidate` - Liquidates an unsafe CDP. This is an unsigned extrinsic that can be called by
//!   anyone, and is typically triggered by an offchain worker.
//! * `settle` - Settles a CDP after a global shutdown. This is also an unsigned extrinsic.
//! * `set_risk_management_params` - Updates the risk management parameters for a collateral type.
//!   This is a privileged extrinsic that can only be called by a specified origin.
//!
//! ### Offchain Worker
//!
//! The pallet includes an offchain worker that monitors the state of all CDPs. If a CDP becomes
//! unsafe, the offchain worker submits an unsigned `liquidate` extrinsic to liquidate the
//! position. In the case of a global shutdown, the offchain worker will submit `settle`
//! extrinsics for all outstanding CDPs.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Encode, MaxEncodedLen};
use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible::{self},
		fungibles::{self, Mutate as FungiblesMutate},
		honzon::{
			CDPTreasury, CDPTreasuryExtended, EmergencyShutdown, LiquidateCollateral, Position,
			Price, PriceProvider, Rate, Ratio, SwapLimit,
		},
		tokens::Preservation,
		UnixTime,
	},
	transactional, PalletId,
};
use frame_system::{offchain::SubmitTransaction, pallet_prelude::*};
use pallet_asset_conversion::Swap;
use pallet_loans::BalanceOf;
use scale_info::TypeInfo;
use sp_runtime::{
	offchain::{
		storage::StorageValueRef,
		storage_lock::{StorageLock, Time},
		Duration,
	},
	traits::{
		AtLeast32BitUnsigned, Bounded, One, Saturating, StaticLookup, UniqueSaturatedInto, Zero,
	},
	transaction_validity::{
		InvalidTransaction, TransactionPriority, TransactionSource, TransactionValidity,
		ValidTransaction,
	},
	DispatchError, DispatchResult, FixedPointNumber, RuntimeDebug,
};
use sp_std::prelude::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;

pub use weights::WeightInfo;

mod types;
pub use types::{CompoundFactor, LiquidateByPriority, RiskManagementParams};
mod impl_risk_manager;
mod impl_vault_provider;

pub use pallet::*;
pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

#[derive(RuntimeDebug)]
pub enum OffchainErr {
	NotValidator,
	OffchainLock,
}

pub const OFFCHAIN_WORKER_DATA: &[u8] = b"acala/cdp-engine/data/";
pub const OFFCHAIN_WORKER_LOCK: &[u8] = b"acala/cdp-engine/lock/";
pub const OFFCHAIN_WORKER_MAX_ITERATIONS: &[u8] = b"acala/cdp-engine/max-iterations/";
pub const LOCK_DURATION: u64 = 100;
pub const DEFAULT_MAX_ITERATIONS: u32 = 1000;

pub type LoansOf<T> = pallet_loans::Pallet<T>;
pub type CurrencyOf<T> = <T as Config>::Currency;

/// Status of CDP
#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq, TypeInfo)]
pub enum CDPStatus {
	Safe,
	Unsafe,
	ChecksFailed(DispatchError),
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	pub type CurrencyIdOf<T> = <T as pallet_loans::Config>::CurrencyId;

	#[pallet::config]
	pub trait Config:
		frame_system::Config
		+ pallet_loans::Config
		+ frame_system::offchain::CreateTransactionBase<Call<Self>>
		+ frame_system::offchain::CreateBare<Call<Self>>
	{
		/// The risk management params
		///
		/// can be set via pallet-parameters
		#[pallet::constant]
		type RiskManagementParams: Get<RiskManagementParams<pallet_loans::BalanceOf<Self>>>;

		/// The default compound factor for all collateral types
		#[pallet::constant]
		type DefaultCompoundFactor: Get<CompoundFactor>;

		/// The minimum debit value to avoid debit dust
		#[pallet::constant]
		type MinimumDebitValue: Get<pallet_loans::BalanceOf<Self>>;

		/// Gets the minimum collateral amount.
		#[pallet::constant]
		type MinimumCollateralAmount: Get<pallet_loans::BalanceOf<Self>>;

		/// Native currency id
		#[pallet::constant]
		type GetNativeCurrencyId: Get<CurrencyIdOf<Self>>;

		/// Stablecoin currency id
		#[pallet::constant]
		type GetStableCurrencyId: Get<CurrencyIdOf<Self>>;

		/// When swap with DEX, the acceptable max slippage for the price from oracle.
		#[pallet::constant]
		type MaxSwapSlippageCompareToOracle: Get<Ratio>;

		/// The CDP treasury to maintain bad debts and surplus generated by CDPs
		type CDPTreasury: CDPTreasuryExtended<
			Self::AccountId,
			Balance = pallet_loans::BalanceOf<Self>,
			CurrencyId = CurrencyIdOf<Self>,
		>;

		/// The price source of all types of currencies related to CDP
		type PriceSource: PriceProvider<CurrencyIdOf<Self>>;

		/// A configuration for base priority of unsigned transactions.
		///
		/// This is exposed so that it can be tuned for particular runtime, when
		/// multiple modules send unsigned transactions.
		#[pallet::constant]
		type UnsignedPriority: Get<TransactionPriority>;

		/// Emergency shutdown.
		type EmergencyShutdown: EmergencyShutdown;

		/// Time used for computing era duration.
		///
		/// It is guaranteed to start being called from the first `on_finalize`.
		/// Thus value at genesis is not used.
		type UnixTime: UnixTime;

		/// Single-currency interface used for loan accounting and collateral holds.
		type Currency: fungible::MutateHold<
				Self::AccountId,
				Balance = pallet_loans::BalanceOf<Self>,
				Reason = <Self as pallet_loans::Config>::RuntimeHoldReason,
			> + fungible::Mutate<Self::AccountId, Balance = pallet_loans::BalanceOf<Self>>;

		/// Multi-currency interface for working with specific assets.
		type Tokens: fungibles::Mutate<
			Self::AccountId,
			AssetId = CurrencyIdOf<Self>,
			Balance = pallet_loans::BalanceOf<Self>,
		>;

		/// The kind of asset which can be used for swapping.
		type AssetKind: Parameter + Member + MaxEncodedLen + Copy + From<CurrencyIdOf<Self>>;

		/// Swap
		type Swap: Swap<
			Self::AccountId,
			Balance = pallet_loans::BalanceOf<Self>,
			AssetKind = Self::AssetKind,
		>;

		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// The unique identifier for a vault.
		type VaultId: Member
			+ Parameter
			+ AtLeast32BitUnsigned
			+ Default
			+ Copy
			+ MaxEncodedLen
			+ Encode;

		/// Weight information for the extrinsics in this module.
		type WeightInfo: WeightInfo;
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The total debit value of specific collateral type already exceed the
		/// hard cap
		ExceedDebitValueHardCap,
		/// The collateral ratio below the required collateral ratio
		BelowRequiredCollateralRatio,
		/// The collateral ratio below the liquidation ratio
		BelowLiquidationRatio,
		/// The CDP must be unsafe status
		MustBeUnsafe,
		/// The CDP must be safe status
		MustBeSafe,
		/// Remain debit value in CDP below the dust amount
		RemainDebitValueTooSmall,
		/// Remain collateral value in CDP below the dust amount.
		/// Withdraw all collateral or leave more than the minimum.
		CollateralAmountBelowMinimum,
		/// Feed price is invalid
		InvalidFeedPrice,
		/// No debit value in CDP so that it cannot be settled
		NoDebitValue,
		/// System has already been shutdown
		AlreadyShutdown,
		/// Must after system shutdown
		MustAfterShutdown,
		/// Collateral in CDP is not enough
		CollateralNotEnough,
		/// debit value decrement is not enough
		NotEnoughDebitDecrement,
		/// Collateral liquidation failed.
		LiquidationFailed,
		/// Invalid rate
		InvalidRate,
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Liquidate the unsafe CDP.
		LiquidateUnsafeCDP {
			owner: AccountIdOf<T>,
			collateral_amount: pallet_loans::BalanceOf<T>,
			bad_debt_value: pallet_loans::BalanceOf<T>,
			target_amount: pallet_loans::BalanceOf<T>,
		},
		/// Settle the CDP has debit.
		SettleCDPInDebit { owner: AccountIdOf<T> },
		/// Directly close CDP has debit by handle debit with DEX.
		CloseCDPInDebitByDEX {
			owner: AccountIdOf<T>,
			sold_collateral_amount: pallet_loans::BalanceOf<T>,
			refund_collateral_amount: pallet_loans::BalanceOf<T>,
			debit_value: pallet_loans::BalanceOf<T>,
		},
		/// The interest rate per sec for specific collateral type updated.
		InterestRatePerSecUpdated { new_interest_rate_per_sec: Option<Rate> },
		/// The liquidation fee for specific collateral type updated.
		LiquidationRatioUpdated { new_liquidation_ratio: Option<Ratio> },
		/// The liquidation penalty rate for specific collateral type updated.
		LiquidationPenaltyUpdated { new_liquidation_penalty: Option<Rate> },
		/// The required collateral penalty rate for specific collateral type updated.
		RequiredCollateralRatioUpdated { new_required_collateral_ratio: Option<Ratio> },
		/// The hard cap of total debit value for specific collateral type updated.
		MaximumTotalDebitValueUpdated { new_total_debit_value: pallet_loans::BalanceOf<T> },
	}

	/// Compound factor of debit balance (PV) to debit value (FV) for a specific stability fee.
	#[pallet::storage]
	#[pallet::getter(fn compound_factor)]
	pub type CompoundFactorStorage<T: Config> =
		StorageMap<_, Twox64Concat, Rate, CompoundFactor, OptionQuery>;

	/// Timestamp in seconds of the last interest accumulation
	///
	/// LastAccumulationSecs: u64
	#[pallet::storage]
	#[pallet::getter(fn last_accumulation_secs)]
	pub type LastAccumulationSecs<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// Reverse mapping from a vault's account ID to its vault ID.
	#[pallet::storage]
	pub type VaultIdByAccountId<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountId, T::VaultId, OptionQuery>;

	/// Stability fee overrides for specific vaults.
	#[pallet::storage]
	#[pallet::getter(fn stability_fee_overrides)]
	pub type StabilityFeeOverrides<T: Config> =
		StorageMap<_, Twox64Concat, T::VaultId, Rate, OptionQuery>;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Issue interest in stable currency for all types of collateral has
		/// debit when block end, and update their debit compound factor
		fn on_initialize(now: BlockNumberFor<T>) -> Weight {
			// only after the block #1, `T::UnixTime::now()` will not report error.
			// https://github.com/paritytech/substrate/blob/4ff92f10058cfe1b379362673dd369e33a919e66/frame/timestamp/src/lib.rs#L276
			// so accumulate interest at the beginning of the block #2
			let now_as_secs: u64 =
				if now > One::one() { T::UnixTime::now().as_secs() } else { Default::default() };
			let reads_writes =
				Self::accumulate_interest(now_as_secs, Self::last_accumulation_secs());
			<T as Config>::WeightInfo::on_initialize().saturating_add(
				T::DbWeight::get().reads_writes(reads_writes as u64, reads_writes as u64),
			)
		}

		/// Runs after every block. Start offchain worker to check CDP and
		/// submit unsigned tx to trigger liquidation or settlement.
		fn offchain_worker(now: BlockNumberFor<T>)
		where
			T: frame_system::offchain::CreateTransactionBase<Call<T>>
				+ frame_system::offchain::CreateBare<Call<T>>,
		{
			if let Err(e) = Self::_offchain_worker() {
				log::info!(
					target: "cdp-engine offchain worker",
					"cannot run offchain worker at {:?}: {:?}",
					now,
					e,
				);
			} else {
				log::debug!(
					target: "cdp-engine offchain worker",
					"offchain worker start at block: {:?} already done!",
					now,
				);
			}
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Liquidate unsafe CDP
		///
		/// The dispatch origin of this call must be _None_.
		///
		/// - `who`: CDP's owner.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::liquidate(<T as Config>::CDPTreasury::max_auction()))]
		pub fn liquidate(
			origin: OriginFor<T>,
			who: <T::Lookup as StaticLookup>::Source,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;
			let who = T::Lookup::lookup(who)?;
			ensure!(!T::EmergencyShutdown::is_shutdown(), Error::<T>::AlreadyShutdown);
			let consumed_weight: Weight = Self::liquidate_unsafe_cdp(who)?;
			Ok(Some(consumed_weight).into())
		}

		/// Settles a CDP with outstanding debit after an emergency shutdown.
		///
		/// The dispatch origin of this call must be _None_.
		///
		/// - `who`: Owner of the CDP to settle.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::settle())]
		pub fn settle(
			origin: OriginFor<T>,
			who: <T::Lookup as StaticLookup>::Source,
		) -> DispatchResult {
			ensure_none(origin)?;
			let who = T::Lookup::lookup(who)?;
			ensure!(T::EmergencyShutdown::is_shutdown(), Error::<T>::MustAfterShutdown);
			Self::settle_cdp_has_debit(who)?;
			Ok(())
		}
	}
	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;

		fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
			match call {
				Call::liquidate { who } => {
					let account = T::Lookup::lookup(who.clone())?;
					let Position { collateral, debit, stability_fee } =
						<LoansOf<T>>::positions(&account);
					if collateral.is_zero() && debit.is_zero() {
						return InvalidTransaction::Stale.into();
					}
					let stability_fee = Rate::from_inner(stability_fee.into_inner());
					if !matches!(
						Self::check_cdp_status(collateral, debit, stability_fee, &account),
						CDPStatus::Unsafe
					) || T::EmergencyShutdown::is_shutdown()
					{
						return InvalidTransaction::Stale.into();
					}

					ValidTransaction::with_tag_prefix("CDPEngineOffchainWorker")
						.priority(T::UnsignedPriority::get())
						.and_provides((<frame_system::Pallet<T>>::block_number(), account))
						.longevity(64_u64)
						.propagate(true)
						.build()
				},
				Call::settle { who } => {
					let account = T::Lookup::lookup(who.clone())?;
					let Position { debit, .. } = <LoansOf<T>>::positions(&account);
					if debit.is_zero() || !T::EmergencyShutdown::is_shutdown() {
						return InvalidTransaction::Stale.into();
					}

					ValidTransaction::with_tag_prefix("CDPEngineOffchainWorker")
						.priority(T::UnsignedPriority::get())
						.and_provides(account)
						.longevity(64_u64)
						.propagate(true)
						.build()
				},
				_ => InvalidTransaction::Call.into(),
			}
		}
	}
}

impl<T: Config> Pallet<T> {
	fn accumulate_interest(now_secs: u64, last_accumulation_secs: u64) -> u32 {
		// Only proceed if the system is not in emergency shutdown and the input timestamp is valid.
		if !T::EmergencyShutdown::is_shutdown() && !now_secs.is_zero() {
			let interval_secs = now_secs.saturating_sub(last_accumulation_secs);
			let mut touched_entries: u32 = 0;

			// Iterate over all debit active stability fee classes with outstanding debt.
			for (stability_fee, total_debits) in pallet_loans::TotalDebitByStabilityFee::<T>::iter()
			{
				// Skip classes with no outstanding debt.
				if total_debits.is_zero() {
					continue;
				}

				// Compute interest rate for the elapsed interval for this stability fee.
				let rate_to_accumulate = Self::compound_interest_rate(stability_fee, interval_secs);
				if rate_to_accumulate.is_zero() {
					continue;
				}

				// Retrieve current compound factor for this fee class.
				let compound_factor = Self::get_compound_factor(stability_fee);
				let compound_factor_increment = compound_factor.saturating_mul(rate_to_accumulate);
				// Calculate the interest to issue
				let stable_coin_balance_to_issue =
					compound_factor_increment.saturating_mul_int(total_debits);

				// Mint and issue the accumulated interest to the system surplus pool.
				let res =
					<T as Config>::CDPTreasury::on_system_surplus(stable_coin_balance_to_issue);
				match res {
					Ok(_) => {
						// Update compound factor in storage to reflect the new accumulated
						// interest.
						let new_compound_factor =
							compound_factor.saturating_add(compound_factor_increment);
						CompoundFactorStorage::<T>::insert(stability_fee, new_compound_factor);
						touched_entries = touched_entries.saturating_add(1);
					},
					Err(e) => {
						// Log the error but allow the loop to continue for other fee classes.
						log::warn!(
							target: "cdp-engine",
							"on_system_surplus: failed to on system surplus {:?}: {:?}. This is unexpected but should be safe",
							stable_coin_balance_to_issue,
							e
						);
					},
				}
			}

			// Record the accumulation timestamp for future accrual runs.
			LastAccumulationSecs::<T>::put(now_secs);
			return touched_entries.saturating_add(1);
		}

		// Always update the accumulation timestamp even if no accrual occurred.
		LastAccumulationSecs::<T>::put(now_secs);
		0
	}

	/// Determines the status (safe, unsafe, or checks-failed) of a CDP position for a given
	/// account.
	///
	/// This function expects that `collateral_amount`, `debit_amount`, and `stability_fee` all
	/// directly reflect the on-chain [`Position`] entry for the provided `who` account. This is
	/// relied upon by all internal call sites and critical for correct risk calculation.
	///
	/// This constraint ensures that collateralization checks always use the up-to-date, canonical
	/// values for each CDP account. Do not call this function with custom or out-of-sync data.
	fn check_cdp_status(
		collateral_amount: BalanceOf<T>,
		debit_amount: BalanceOf<T>,
		stability_fee: Rate,
		who: &AccountIdOf<T>,
	) -> CDPStatus {
		let currency_id = T::GetNativeCurrencyId::get();
		let stable_currency_id = T::GetStableCurrencyId::get();
		// Calculate relative price
		let feed_price = match T::PriceSource::get_relative_price(currency_id, stable_currency_id) {
			Some(price) => price,
			None => return CDPStatus::ChecksFailed(Error::<T>::InvalidFeedPrice.into()),
		};
		// Calculate compound factor
		let stability_fee = Self::get_effective_stability_fee(stability_fee, who);
		let compound_factor = Self::get_compound_factor(stability_fee);
		let collateral_ratio = Self::calculate_collateral_ratio(
			collateral_amount,
			debit_amount,
			feed_price,
			compound_factor,
		);
		if collateral_ratio < Self::get_liquidation_ratio() {
			CDPStatus::Unsafe
		} else {
			CDPStatus::Safe
		}
	}

	/// Get the effective stability fee for a given account.
	///
	/// The effective stability fee for an account is the lowest value
	/// among the position's stored stability fee, the current global stability fee, and any
	/// applicable per-vault override.
	fn get_effective_stability_fee(stability_fee: Rate, who: &AccountIdOf<T>) -> Rate {
		let current_stability_fee = Self::interest_rate_per_sec();
		let mut effective_fee = sp_std::cmp::min(stability_fee, current_stability_fee);

		if let Some(vault_id) = VaultIdByAccountId::<T>::get(who) {
			if let Some(override_fee) = Self::stability_fee_overrides(vault_id) {
				effective_fee = sp_std::cmp::min(effective_fee, override_fee);
			}
		}

		effective_fee
	}

	/// Calculates the compounded interest rate for a given rate per second over a specified number
	/// of seconds.
	///
	/// The formula is:
	/// `(1 + rate_per_sec).pow(secs) - 1`
	pub fn compound_interest_rate(rate_per_sec: Rate, secs: u64) -> Rate {
		rate_per_sec
			.saturating_add(Rate::one())
			.saturating_pow(secs.unique_saturated_into())
			.saturating_sub(Rate::one())
	}

	/// Retrieve the compound factor associated with the given stability fee.
	///
	/// This function ensures the correct factor is available in storage,
	/// initializing it with the default if not yet set.
	pub fn get_compound_factor(stability_fee: Rate) -> CompoundFactor {
		if let Some(compound_factor) = CompoundFactorStorage::<T>::get(stability_fee) {
			compound_factor
		} else {
			let default_rate = T::DefaultCompoundFactor::get();
			CompoundFactorStorage::<T>::insert(stability_fee, default_rate);
			default_rate
		}
	}

	/// Converts a given debit balance to its value based on the stability fee's compound factor.
	pub fn convert_to_debit_value(
		debit_balance: BalanceOf<T>,
		stability_fee: Rate,
	) -> BalanceOf<T> {
		Self::get_compound_factor(stability_fee).saturating_mul_int(debit_balance)
	}

	/// Converts a debit value amount into a debit balance using the given stability fee's compound
	/// factor
	pub fn convert_to_debit_balance(
		debit_value: BalanceOf<T>,
		stability_fee: Rate,
	) -> BalanceOf<T> {
		Self::get_compound_factor(stability_fee)
			.reciprocal()
			.saturating_mul_int(debit_value)
	}

	pub(crate) fn average_compound_factor() -> CompoundFactor {
		let mut total_debit = BalanceOf::<T>::zero();
		let mut total_value = BalanceOf::<T>::zero();

		for (stability_fee, debit_balance) in pallet_loans::TotalDebitByStabilityFee::<T>::iter() {
			if debit_balance.is_zero() {
				continue;
			}
			let rate = Self::get_compound_factor(stability_fee);
			total_debit = total_debit.saturating_add(debit_balance);
			total_value = total_value.saturating_add(rate.saturating_mul_int(debit_balance));
		}

		if total_debit.is_zero() {
			T::DefaultCompoundFactor::get()
		} else {
			CompoundFactor::checked_from_rational(total_value, total_debit)
				.unwrap_or_else(T::DefaultCompoundFactor::get)
		}
	}

	/// Calculates the collateral ratio given collateral, debit, price, and compound factor.
	pub fn calculate_collateral_ratio(
		collateral_balance: BalanceOf<T>,
		debit_balance: BalanceOf<T>,
		price: Price,
		compound_factor: CompoundFactor,
	) -> Ratio {
		let locked_collateral_value = price.saturating_mul_int(collateral_balance);
		let debit_value = compound_factor.saturating_mul_int(debit_balance);

		// if debit_value is zero, collateral ratio is max value
		Ratio::checked_from_rational(locked_collateral_value, debit_value)
			.unwrap_or_else(Ratio::max_value)
	}

	pub fn adjust_position(
		who: &AccountIdOf<T>,
		collateral_adjustment: pallet_loans::BalanceAdjustment<pallet_loans::BalanceOf<T>>,
		debit_adjustment: pallet_loans::BalanceAdjustment<pallet_loans::BalanceOf<T>>,
		maybe_new_stability_fee: Option<Rate>,
	) -> DispatchResult {
		<LoansOf<T>>::adjust_position(
			who,
			collateral_adjustment,
			debit_adjustment,
			maybe_new_stability_fee,
		)?;
		Ok(())
	}

	pub fn adjust_position_by_debit_value(
		who: &AccountIdOf<T>,
		collateral_adjustment: pallet_loans::BalanceAdjustment<pallet_loans::BalanceOf<T>>,
		debit_value_adjustment: pallet_loans::BalanceAdjustment<pallet_loans::BalanceOf<T>>,
	) -> DispatchResult {
		let Position { debit, stability_fee, .. } = <LoansOf<T>>::positions(who);
		let position_stability_fee = Rate::from_inner(stability_fee.into_inner());
		let effective_stability_fee =
			Self::get_effective_stability_fee(position_stability_fee, who);

		match &debit_value_adjustment {
			pallet_loans::BalanceAdjustment::Decrease(amount) => {
				let debit_adjustment_abs =
					Self::convert_to_debit_balance(*amount, effective_stability_fee);
				let actual_adjustment_abs = debit.min(debit_adjustment_abs);
				let debit_adjustment =
					pallet_loans::BalanceAdjustment::decrease(actual_adjustment_abs);

				Self::adjust_position(who, collateral_adjustment, debit_adjustment, None)?;
			},
			pallet_loans::BalanceAdjustment::Increase(amount) => {
				let new_stability_fee = Self::interest_rate_per_sec();
				let debit_adjustment_abs =
					Self::convert_to_debit_balance(*amount, new_stability_fee);
				let debit_adjustment =
					pallet_loans::BalanceAdjustment::increase(debit_adjustment_abs);

				Self::adjust_position(
					who,
					collateral_adjustment,
					debit_adjustment,
					Some(new_stability_fee),
				)?;
			},
		}

		Ok(())
	}

	// --- Shared validation helpers for risk management (trait + internal) ---
	/// Shared validator for position collateral/debt, reused by both trait and pallet logic.
	fn do_check_position(
		collateral_balance: BalanceOf<T>,
		debit_balance: BalanceOf<T>,
		check_required_ratio: bool,
	) -> Result<(), Error<T>> {
		// If there is debit, check the collateral ratio
		if !debit_balance.is_zero() {
			let compound_factor = Self::average_compound_factor();
			let debit_value = compound_factor.saturating_mul_int(debit_balance);
			let feed_price = <T as Config>::PriceSource::get_relative_price(
				T::GetNativeCurrencyId::get(),
				T::GetStableCurrencyId::get(),
			)
			.ok_or(Error::<T>::InvalidFeedPrice)?;
			let collateral_ratio = Self::calculate_collateral_ratio(
				collateral_balance,
				debit_balance,
				feed_price,
				compound_factor,
			);

			// Check collateral ratio >= required collateral ratio
			if check_required_ratio {
				if let Some(required_collateral_ratio) = Self::required_collateral_ratio() {
					ensure!(
						collateral_ratio >= required_collateral_ratio,
						Error::<T>::BelowRequiredCollateralRatio
					);
				}
			}

			// Check collateral ratio >= liquidation ratio
			let liquidation_ratio = Self::get_liquidation_ratio();
			ensure!(collateral_ratio >= liquidation_ratio, Error::<T>::BelowLiquidationRatio);

			ensure!(
				debit_value >= T::MinimumDebitValue::get(),
				Error::<T>::RemainDebitValueTooSmall,
			);
		} else if !collateral_balance.is_zero() {
			ensure!(
				collateral_balance >= T::MinimumCollateralAmount::get(),
				Error::<T>::CollateralAmountBelowMinimum,
			);
		}

		Ok(())
	}

	/// Shared checker for cap on aggregate debits.
	fn do_check_debit_cap(total_debit_balance: BalanceOf<T>) -> Result<(), Error<T>> {
		let hard_cap = Self::maximum_total_debit_value();
		let mut total_debit_value = BalanceOf::<T>::zero();
		for (stability_fee, debit_balance) in pallet_loans::TotalDebitByStabilityFee::<T>::iter() {
			if debit_balance.is_zero() {
				continue;
			}
			let compound_factor = Self::get_compound_factor(stability_fee);
			total_debit_value =
				total_debit_value.saturating_add(compound_factor.saturating_mul_int(debit_balance));
		}
		if total_debit_value.is_zero() && !total_debit_balance.is_zero() {
			let compound_factor = Self::average_compound_factor();
			total_debit_value = compound_factor.saturating_mul_int(total_debit_balance);
		}
		ensure!(total_debit_value <= hard_cap, Error::<T>::ExceedDebitValueHardCap);
		Ok(())
	}

	/// Generate new debit in advance, buy collateral and deposit it into CDP,
	/// and the collateral ratio will be reduced but CDP must still be at valid risk.
	#[transactional]
	pub fn expand_position_collateral(
		who: &AccountIdOf<T>,
		increase_debit_value: BalanceOf<T>,
		min_increase_collateral: BalanceOf<T>,
	) -> DispatchResult {
		let currency_id = T::GetNativeCurrencyId::get();
		let loans_module_account = <LoansOf<T>>::account_id();

		// issue stable coin in advance
		<T as Config>::CDPTreasury::issue_debit(&loans_module_account, increase_debit_value, true)?;

		// swap stable coin to collateral
		let increase_collateral = T::Swap::swap_exact_tokens_for_tokens(
			loans_module_account.clone(),
			vec![T::GetStableCurrencyId::get().into(), currency_id.into()],
			increase_debit_value,
			Some(min_increase_collateral),
			loans_module_account.clone(),
			true,
		)?;

		// update CDP state
		let collateral_adjustment = pallet_loans::BalanceAdjustment::increase(increase_collateral);
		let new_stability_fee = Self::interest_rate_per_sec();
		let increase_debit_balance =
			Self::convert_to_debit_balance(increase_debit_value, new_stability_fee);
		let debit_adjustment = pallet_loans::BalanceAdjustment::increase(increase_debit_balance);
		Self::adjust_position(
			who,
			collateral_adjustment,
			debit_adjustment,
			Some(new_stability_fee),
		)?;

		let Position { collateral, debit, .. } = <LoansOf<T>>::positions(who);
		// check the CDP if is still at valid risk
		Self::do_check_position(collateral, debit, false)?;
		// debit cap check due to new issued stable coin
		let Position { debit: total_debit, .. } = <LoansOf<T>>::total_positions();
		Self::do_check_debit_cap(total_debit)?;
		Ok(())
	}

	/// Sell the collateral locked in CDP to get stable coin to repay the debit,
	/// and the collateral ratio will be increased.
	#[transactional]
	fn shrink_position_debit(
		who: &AccountIdOf<T>,
		decrease_collateral: BalanceOf<T>,
		min_decrease_debit_value: BalanceOf<T>,
	) -> DispatchResult {
		let currency_id = T::GetNativeCurrencyId::get();
		let loans_module_account = <LoansOf<T>>::account_id();
		let stable_currency_id = T::GetStableCurrencyId::get();
		let Position { collateral, debit, stability_fee } = <LoansOf<T>>::positions(who);
		let position_stability_fee = Rate::from_inner(stability_fee.into_inner());
		let effective_stability_fee =
			Self::get_effective_stability_fee(position_stability_fee, who);

		// ensure collateral of CDP is enough
		ensure!(decrease_collateral <= collateral, Error::<T>::CollateralNotEnough);

		// swap collateral to stable coin
		let actual_stable_amount = T::Swap::swap_exact_tokens_for_tokens(
			loans_module_account.clone(),
			vec![currency_id.into(), stable_currency_id.into()],
			decrease_collateral,
			Some(min_decrease_debit_value),
			loans_module_account.clone(),
			true,
		)?;

		// update CDP state
		let collateral_adjustment = pallet_loans::BalanceAdjustment::decrease(decrease_collateral);
		let previous_debit_value = Self::convert_to_debit_value(debit, effective_stability_fee);
		let (decrease_debit_value, decrease_debit_balance) =
			if actual_stable_amount >= previous_debit_value {
				// refund extra stable coin to the CDP owner
				<T as Config>::Tokens::transfer(
					stable_currency_id,
					&loans_module_account,
					who,
					actual_stable_amount.saturating_sub(previous_debit_value),
					Preservation::Protect,
				)?;

				(previous_debit_value, debit)
			} else {
				(
					actual_stable_amount,
					Self::convert_to_debit_balance(actual_stable_amount, effective_stability_fee),
				)
			};

		let debit_adjustment = pallet_loans::BalanceAdjustment::decrease(decrease_debit_balance);
		Self::adjust_position(who, collateral_adjustment, debit_adjustment, None)?;

		// repay the debit of CDP
		<T as Config>::CDPTreasury::burn_debit(&loans_module_account, decrease_debit_value)?;

		// check the CDP if is still at valid risk.
		let Position { collateral: updated_collateral, debit: updated_debit, .. } =
			<LoansOf<T>>::positions(who);
		Self::do_check_position(updated_collateral, updated_debit, false)?;
		Ok(())
	}

	// settle cdp has debit when emergency shutdown
	pub fn settle_cdp_has_debit(who: AccountIdOf<T>) -> DispatchResult {
		let currency_id = T::GetNativeCurrencyId::get();
		let Position { collateral, debit, stability_fee } = <LoansOf<T>>::positions(&who);
		let stability_fee = Rate::from_inner(stability_fee.into_inner());
		let stability_fee = Self::get_effective_stability_fee(stability_fee, &who);
		ensure!(!debit.is_zero(), Error::<T>::NoDebitValue);

		// confiscate collateral in cdp to cdp treasury
		// and decrease CDP's debit to zero
		let settle_price: Price =
			T::PriceSource::get_relative_price(T::GetStableCurrencyId::get(), currency_id)
				.ok_or(Error::<T>::InvalidFeedPrice)?;
		let bad_debt_value = Self::convert_to_debit_value(debit, stability_fee);
		let confiscate_collateral_amount =
			sp_std::cmp::min(settle_price.saturating_mul_int(bad_debt_value), collateral);

		// confiscate collateral and all debit
		<LoansOf<T>>::confiscate_collateral_and_debit(&who, confiscate_collateral_amount, debit)?;

		Self::deposit_event(Event::SettleCDPInDebit { owner: who });
		Ok(())
	}

	// close cdp has debit by swap collateral to exact debit
	#[transactional]
	pub fn close_cdp_has_debit_by_dex(
		who: AccountIdOf<T>,
		max_collateral_amount: BalanceOf<T>,
	) -> DispatchResult {
		let Position { collateral, debit, stability_fee } = <LoansOf<T>>::positions(&who);
		let stability_fee = Rate::from_inner(stability_fee.into_inner());
		ensure!(!debit.is_zero(), Error::<T>::NoDebitValue);
		ensure!(
			matches!(
				Self::check_cdp_status(collateral, debit, stability_fee, &who),
				CDPStatus::Safe
			),
			Error::<T>::MustBeSafe
		);
		let stability_fee = Self::get_effective_stability_fee(stability_fee, &who);

		// confiscate all collateral and debit of unsafe cdp to cdp treasury
		<LoansOf<T>>::confiscate_collateral_and_debit(&who, collateral, debit)?;

		// swap exact stable with DEX in limit of price impact
		let debit_value = Self::convert_to_debit_value(debit, stability_fee);
		let collateral_supply = collateral.min(max_collateral_amount);

		let (actual_supply_collateral, _) = <T as Config>::CDPTreasury::swap_collateral_to_stable(
			SwapLimit::ExactTarget(collateral_supply, debit_value),
			false,
		)?;

		// refund remain collateral to CDP owner
		let refund_collateral_amount = collateral
			.checked_sub(&actual_supply_collateral)
			.expect("swap success means collateral >= actual_supply_collateral; qed");
		<T as Config>::CDPTreasury::withdraw_collateral(&who, refund_collateral_amount)?;

		Self::deposit_event(Event::CloseCDPInDebitByDEX {
			owner: who,
			sold_collateral_amount: actual_supply_collateral,
			refund_collateral_amount,
			debit_value,
		});
		Ok(())
	}

	/// Liquidate an unsafe CDP.
	///
	/// 1. Ensure the CDP is unsafe
	/// 2. Confiscate all collateral and debit of unsafe CDP to CDP treasury
	/// 3. Convert the debit to stablecoin
	/// 4. Liquidate the collateral by priority
	/// 5. Deposit the event
	/// 6. Return the weight
	pub fn liquidate_unsafe_cdp(who: AccountIdOf<T>) -> Result<Weight, DispatchError> {
		let Position { collateral, debit, stability_fee } = <LoansOf<T>>::positions(&who);
		let stability_fee = Rate::from_inner(stability_fee.into_inner());

		// ensure the cdp is unsafe
		ensure!(
			matches!(
				Self::check_cdp_status(collateral, debit, stability_fee, &who),
				CDPStatus::Unsafe
			),
			Error::<T>::MustBeUnsafe
		);
		let stability_fee = Self::get_effective_stability_fee(stability_fee, &who);

		// confiscate all collateral and debit of unsafe cdp to cdp treasury
		<LoansOf<T>>::confiscate_collateral_and_debit(&who, collateral, debit)?;

		let bad_debt_value = Self::convert_to_debit_value(debit, stability_fee);
		let liquidation_penalty = Self::get_liquidation_penalty();
		let target_stable_amount = liquidation_penalty.saturating_mul_acc_int(bad_debt_value);

		Self::handle_liquidated_collateral(&who, collateral, target_stable_amount)?;

		Self::deposit_event(Event::LiquidateUnsafeCDP {
			owner: who,
			collateral_amount: collateral,
			bad_debt_value,
			target_amount: target_stable_amount,
		});
		Ok(T::WeightInfo::liquidate(1))
	}

	/// Handle liquidated collateral by priority
	///
	/// 1. If target stable amount is zero, refund collateral to CDP owner
	/// 2. If target stable amount is not zero, liquidate collateral by priority
	pub fn handle_liquidated_collateral(
		who: &AccountIdOf<T>,
		amount: BalanceOf<T>,
		target_stable_amount: BalanceOf<T>,
	) -> DispatchResult {
		let currency_id = T::GetNativeCurrencyId::get();
		if target_stable_amount.is_zero() {
			// refund collateral to CDP owner
			if !amount.is_zero() {
				<T as Config>::CDPTreasury::withdraw_collateral(who, amount)?;
			}
			return Ok(());
		}
		LiquidateByPriority::<T>::liquidate(who, currency_id, amount, target_stable_amount)
	}
}

// Helper functions for risk management parameters
impl<T: Config> Pallet<T> {
	fn risk_management_params() -> RiskManagementParams<pallet_loans::BalanceOf<T>> {
		T::RiskManagementParams::get()
	}

	/// Returns the maximum total debit value that can be issued for the current collateral type.
	fn maximum_total_debit_value() -> BalanceOf<T> {
		let params = Self::risk_management_params();
		params.maximum_total_debit_value
	}

	/// Returns the required collateral ratio, if set, for the current collateral type.
	fn required_collateral_ratio() -> Option<Ratio> {
		let params = Self::risk_management_params();
		params.required_collateral_ratio
	}

	/// Returns the interest rate per second for the current collateral type.
	fn interest_rate_per_sec() -> Rate {
		let params = Self::risk_management_params();
		// default value is 0
		params.interest_rate_per_sec
	}

	/// Returns the liquidation ratio for the current collateral type.
	fn get_liquidation_ratio() -> Ratio {
		let params = Self::risk_management_params();
		params.liquidation_ratio
	}

	/// Returns the current liquidation penalty rate for the collateral type, or the default if not
	/// set.
	fn get_liquidation_penalty() -> Rate {
		let params = Self::risk_management_params();
		params.liquidation_penalty
	}
}

// Unsigned implementations
impl<T: Config> Pallet<T> {
	fn submit_unsigned_liquidation_tx(who: AccountIdOf<T>) {
		let who = T::Lookup::unlookup(who);
		let call = Call::<T>::liquidate { who: who.clone() };
		let xt = T::create_bare(call.into());
		if SubmitTransaction::<T, Call<T>>::submit_transaction(xt).is_err() {
			log::info!(
				target: "cdp-engine offchain worker",
				"submit unsigned liquidation tx for \nCDP - AccountId {:?} \nfailed!",
				who,
			);
		}
	}

	fn submit_unsigned_settlement_tx(who: AccountIdOf<T>) {
		let who = T::Lookup::unlookup(who);
		let call = Call::<T>::settle { who: who.clone() };
		let xt = T::create_bare(call.into());
		if SubmitTransaction::<T, Call<T>>::submit_transaction(xt).is_err() {
			log::info!(
				target: "cdp-engine offchain worker",
				"submit unsigned settlement tx for \nCDP - AccountId {:?} \nfailed!",
				who,
			);
		}
	}

	fn _offchain_worker() -> Result<(), OffchainErr> {
		if !sp_io::offchain::is_validator() {
			return Err(OffchainErr::NotValidator);
		}

		let lock_expiration = Duration::from_millis(LOCK_DURATION);
		let mut lock = StorageLock::<Time>::with_deadline(OFFCHAIN_WORKER_LOCK, lock_expiration);
		let mut guard = lock.try_lock().map_err(|_| OffchainErr::OffchainLock)?;
		let to_be_continue = StorageValueRef::persistent(OFFCHAIN_WORKER_DATA);

		let start_key: Option<Vec<u8>> = if let Ok(Some(maybe_last_iterator_previous_key)) =
			to_be_continue.get::<Option<Vec<u8>>>()
		{
			maybe_last_iterator_previous_key
		} else {
			None
		};

		let max_iterations = StorageValueRef::persistent(OFFCHAIN_WORKER_MAX_ITERATIONS)
			.get::<u32>()
			.unwrap_or(Some(DEFAULT_MAX_ITERATIONS))
			.unwrap_or(DEFAULT_MAX_ITERATIONS);

		let currency_id = T::GetNativeCurrencyId::get();
		let is_shutdown = T::EmergencyShutdown::is_shutdown();

		let mut map_iterator = match start_key.clone() {
			Some(key) => <pallet_loans::Positions<T>>::iter_from(key),
			None => <pallet_loans::Positions<T>>::iter(),
		};

		let mut finished = true;
		let mut iteration_count = 0;
		let iteration_start_time = sp_io::offchain::timestamp();

		while let Some((who, Position { collateral, debit, stability_fee })) = map_iterator.next() {
			let effective_stability_fee = Self::get_effective_stability_fee(stability_fee, &who);
			if !is_shutdown &&
				matches!(
					Self::check_cdp_status(collateral, debit, effective_stability_fee, &who),
					CDPStatus::Unsafe
				) {
				Self::submit_unsigned_liquidation_tx(who);
			} else if is_shutdown && !debit.is_zero() {
				Self::submit_unsigned_settlement_tx(who);
			}

			iteration_count += 1;
			if iteration_count == max_iterations {
				finished = false;
				break;
			}
			guard.extend_lock().map_err(|_| OffchainErr::OffchainLock)?;
		}
		let iteration_end_time = sp_io::offchain::timestamp();
		log::debug!(
			target: "cdp-engine offchain worker",
			"iteration info: max_iterations: {:?}, currency_id: {:?}, start_key: {:?}, iteration_count: {:?}, start_at: {:?}, end_at: {:?}, execution_time: {:?}",
			max_iterations,
			currency_id,
			start_key,
			iteration_count,
			iteration_start_time,
			iteration_end_time,
			iteration_end_time.diff(&iteration_start_time)
		);

		if finished {
			to_be_continue.set(&Option::<Vec<u8>>::None);
		} else {
			to_be_continue.set(&Some(map_iterator.last_raw_key()));
		}

		guard.forget();

		Ok(())
	}
}
