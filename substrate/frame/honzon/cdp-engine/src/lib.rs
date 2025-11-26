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
mod pallet_impls;
mod trait_impls;

pub use pallet::*;
pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
pub type LoansOf<T> = pallet_loans::Pallet<T>;
pub type CurrencyOf<T> = <T as Config>::Currency;
pub type CurrencyIdOf<T> = <T as Config>::CurrencyId;

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

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config:
		frame_system::Config
		+ pallet_loans::Config
		+ frame_system::offchain::CreateTransactionBase<Call<Self>>
		+ frame_system::offchain::CreateBare<Call<Self>>
	{
		type CurrencyId: Parameter + Member + Copy + MaybeSerializeDeserialize + Ord;
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
		/// Liquidates an unsafe CDP by confiscating its collateral and debt, then selling the
		/// collateral to cover the debt plus a liquidation penalty.
		///
		/// ## Dispatch Origin
		///
		/// The dispatch origin of this call must be _None_ (unsigned extrinsic). This call can be
		/// submitted by anyone and is typically triggered by an offchain worker that monitors CDP
		/// safety status.
		///
		/// ## Details
		///
		/// This function will only succeed if the CDP owned by `who` is in an unsafe state (i.e.,
		/// its collateral-to-debt ratio has fallen below the liquidation threshold). During
		/// liquidation:
		///
		/// 1. The CDP's collateral and debt are confiscated to the CDP treasury
		/// 2. The debt value is converted to stablecoin value including accumulated stability fees
		/// 3. The collateral is liquidated (sold via DEX) to cover the debt plus a liquidation
		///    penalty
		/// 4. Any remaining collateral after covering the target amount may be refunded to the CDP
		///    owner
		///
		/// The function will fail if the system is in emergency shutdown mode (use [`settle`]
		/// instead in that case).
		///
		/// ## Parameters
		///
		/// - `who`: The account ID of the CDP owner to liquidate.
		///
		/// ## Errors
		///
		/// - [`Error::AlreadyShutdown`]: System is in emergency shutdown mode.
		/// - [`Error::InvalidFeedPrice`]: Feed price is invalid.
		/// - [`Error::RemainDebitValueTooSmall`]: Remain debit value in CDP below the dust amount.
		/// - [`Error::ExceedDebitValueHardCap`]: The total debit value of specific collateral type
		///   already exceed the hard cap.
		///
		/// ## Events
		///
		/// - [`Event::LiquidateUnsafeCDP`]: Emitted when a CDP is successfully liquidated.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::liquidate(<T as Config>::CDPTreasury::max_auction()))]
		pub fn liquidate(
			origin: OriginFor<T>,
			who: <T::Lookup as StaticLookup>::Source,
		) -> DispatchResultWithPostInfo {
			ensure_none(origin)?;
			let who = T::Lookup::lookup(who)?;
			ensure!(!T::EmergencyShutdown::is_shutdown(), Error::<T>::AlreadyShutdown);
			let consumed_weight: Weight = Self::do_liquidate(who)?;
			Ok(Some(consumed_weight).into())
		}

		/// Settles a CDP with outstanding debit after an emergency shutdown.
		///
		/// ## Dispatch Origin
		///
		/// The dispatch origin of this call must be _None_ (unsigned extrinsic). This call can be
		/// submitted by anyone and is typically triggered by an offchain worker that monitors CDP
		/// safety status.
		///
		/// ## Details
		///
		/// This function will only succeed if the CDP owned by `who` has outstanding debit after an
		/// emergency shutdown. During settlement:
		///
		/// - `who`: Owner of the CDP to settle.
		///
		/// ## Errors
		///
		/// - [`Error::MustAfterShutdown`]: System is not in emergency shutdown mode.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::settle())]
		pub fn settle(
			origin: OriginFor<T>,
			who: <T::Lookup as StaticLookup>::Source,
		) -> DispatchResult {
			ensure_none(origin)?;
			let who = T::Lookup::lookup(who)?;
			ensure!(T::EmergencyShutdown::is_shutdown(), Error::<T>::MustAfterShutdown);
			Self::do_settle(who)?;
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
					let Position { collateral, debit } = <LoansOf<T>>::positions(&account);
					if collateral.is_zero() && debit.units.is_zero() {
						return InvalidTransaction::Stale.into();
					}
					if Self::is_cdp_safe(collateral, debit.units, debit.stability_fee)
						.unwrap_or(true) || T::EmergencyShutdown::is_shutdown()
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
					if debit.units.is_zero() || !T::EmergencyShutdown::is_shutdown() {
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
