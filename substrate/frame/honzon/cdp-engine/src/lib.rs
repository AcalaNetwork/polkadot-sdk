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

//! # CDP Engine Module
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
//! * `set_collateral_params` - Updates the risk management parameters for a collateral type. This
//!   is a privileged extrinsic that can only be called by a specified origin.
//!
//! ### Offchain Worker
//!
//! The pallet includes an offchain worker that monitors the state of all CDPs. If a CDP becomes
//! unsafe, the offchain worker submits an unsigned `liquidate` extrinsic to liquidate the
//! position. In the case of a global shutdown, the offchain worker will submit `settle`
//! extrinsics for all outstanding CDPs.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::unused_unit)]
#![allow(clippy::upper_case_acronyms)]

use codec::MaxEncodedLen;
use frame_support::{
	pallet_prelude::*,
	traits::{fungibles, UnixTime},
	transactional, PalletId,
};
use frame_system::{offchain::SubmitTransaction, pallet_prelude::*};
use pallet_loans::BalanceOf;
use pallet_traits::{
	CDPTreasury, CDPTreasuryExtended, Change, DEXManager, EmergencyShutdown, ExchangeRate,
	FractionalRate, LiquidateCollateral, Position, Price, PriceProvider, Rate, Ratio, RiskManager,
	Swap, SwapLimit,
};
use scale_info::TypeInfo;
use sp_runtime::{
	offchain::{
		storage::StorageValueRef,
		storage_lock::{StorageLock, Time},
		Duration,
	},
	traits::{
		AccountIdConversion, Bounded, One, Saturating, StaticLookup, UniqueSaturatedInto, Zero,
	},
	transaction_validity::{
		InvalidTransaction, TransactionPriority, TransactionSource, TransactionValidity,
		ValidTransaction,
	},
	DispatchError, DispatchResult, FixedPointNumber, RuntimeDebug,
};
use sp_std::{marker::PhantomData, prelude::*};

mod mock;
mod tests;
pub mod weights;

pub use pallet::*;
pub use weights::WeightInfo;

pub type CurrencyId = u32;
pub type Amount = i128;

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

/// Risk management params
#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq, Default, TypeInfo, MaxEncodedLen)]
pub struct RiskManagementParams<Balance> {
	/// Maximum total debit value generated from it, when reach the hard
	/// cap, CDP's owner cannot issue more stablecoin under the collateral
	/// type.
	pub maximum_total_debit_value: Balance,

	/// Extra interest rate per sec, `None` value means not set
	pub interest_rate_per_sec: Option<FractionalRate>,

	/// Liquidation ratio, when the collateral ratio of
	/// CDP under this collateral type is below the liquidation ratio, this
	/// CDP is unsafe and can be liquidated. `None` value means not set
	pub liquidation_ratio: Option<Ratio>,

	/// Liquidation penalty rate, when liquidation occurs,
	/// CDP will be deducted an additional penalty base on the product of
	/// penalty rate and debit value. `None` value means not set
	pub liquidation_penalty: Option<FractionalRate>,

	/// Required collateral ratio, if it's set, cannot adjust the position
	/// of CDP so that the current collateral ratio is lower than the
	/// required collateral ratio. `None` value means not set
	pub required_collateral_ratio: Option<Ratio>,
}

// typedef to help polkadot.js disambiguate Change with different generic
// parameters
type ChangeOptionRate = Change<Option<Rate>>;
type ChangeOptionRatio = Change<Option<Ratio>>;

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

	#[pallet::config]
	pub trait Config: frame_system::Config + pallet_loans::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The origin which may update risk management parameters. Root can
		/// always do this.
		type UpdateOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The default liquidation ratio for all collateral types of CDP
		#[pallet::constant]
		type DefaultLiquidationRatio: Get<Ratio>;

		/// The default debit exchange rate for all collateral types
		#[pallet::constant]
		type DefaultDebitExchangeRate: Get<ExchangeRate>;

		/// The default liquidation penalty rate when liquidate unsafe CDP
		#[pallet::constant]
		type DefaultLiquidationPenalty: Get<FractionalRate>;

		/// The minimum debit value to avoid debit dust
		#[pallet::constant]
		type MinimumDebitValue: Get<pallet_loans::BalanceOf<Self>>;

		/// Gets the minimum collateral amount.
		#[pallet::constant]
		type MinimumCollateralAmount: Get<pallet_loans::BalanceOf<Self>>;

		/// Native currency id
		#[pallet::constant]
		type GetNativeCurrencyId: Get<CurrencyId>;

		/// Stablecoin currency id
		#[pallet::constant]
		type GetStableCurrencyId: Get<CurrencyId>;

		/// When swap with DEX, the acceptable max slippage for the price from oracle.
		#[pallet::constant]
		type MaxSwapSlippageCompareToOracle: Get<Ratio>;

		/// The CDP treasury to maintain bad debts and surplus generated by CDPs
		type CDPTreasury: CDPTreasuryExtended<
			Self::AccountId,
			Balance = pallet_loans::BalanceOf<Self>,
			CurrencyId = CurrencyId,
		>;

		/// The price source of all types of currencies related to CDP
		type PriceSource: PriceProvider<CurrencyId>;

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

		/// Currency for transfer assets
		type Currency: fungibles::Mutate<
			Self::AccountId,
			AssetId = CurrencyId,
			Balance = pallet_loans::BalanceOf<Self>,
		>;

		/// Dex
		type DEX: DEXManager<Self::AccountId, pallet_loans::BalanceOf<Self>, CurrencyId>;

		/// Swap
		type Swap: Swap<Self::AccountId, pallet_loans::BalanceOf<Self>, CurrencyId>;

		#[pallet::constant]
		type PalletId: Get<PalletId>;

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
		/// convert debit value to debit balance failed
		ConvertDebitBalanceFailed,
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
			owner: T::AccountId,
			collateral_amount: pallet_loans::BalanceOf<T>,
			bad_debt_value: pallet_loans::BalanceOf<T>,
			target_amount: pallet_loans::BalanceOf<T>,
		},
		/// Settle the CDP has debit.
		SettleCDPInDebit { owner: T::AccountId },
		/// Directly close CDP has debit by handle debit with DEX.
		CloseCDPInDebitByDEX {
			owner: T::AccountId,
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

	/// Exchange rate of debit units and debit value for a specific stability fee.
	#[pallet::storage]
	#[pallet::getter(fn debit_exchange_rate)]
	pub type DebitExchangeRate<T: Config> =
		StorageMap<_, Twox64Concat, Rate, ExchangeRate, OptionQuery>;

	/// Risk management params.
	#[pallet::storage]
	#[pallet::getter(fn collateral_params)]
	pub type CollateralParams<T: Config> =
		StorageValue<_, RiskManagementParams<pallet_loans::BalanceOf<T>>, ValueQuery>;

	/// Timestamp in seconds of the last interest accumulation
	///
	/// LastAccumulationSecs: u64
	#[pallet::storage]
	#[pallet::getter(fn last_accumulation_secs)]
	pub type LastAccumulationSecs<T: Config> = StorageValue<_, u64, ValueQuery>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub collateral_params:
			(Option<Rate>, Option<Ratio>, Option<Rate>, Option<Ratio>, pallet_loans::BalanceOf<T>),
		pub _phantom: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			let (
				interest_rate_per_sec,
				liquidation_ratio,
				liquidation_penalty,
				required_collateral_ratio,
				maximum_total_debit_value,
			) = self.collateral_params;
			CollateralParams::<T>::put(RiskManagementParams {
				maximum_total_debit_value,
				interest_rate_per_sec: interest_rate_per_sec.map(|v| {
					FractionalRate::try_from(v).expect("interest_rate_per_sec out of bound")
				}),
				liquidation_ratio,
				liquidation_penalty: liquidation_penalty.map(|v| {
					FractionalRate::try_from(v).expect("liquidation_penalty out of bound")
				}),
				required_collateral_ratio,
			});
		}
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Issue interest in stable currency for all types of collateral has
		/// debit when block end, and update their debit exchange rate
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
		fn offchain_worker(now: BlockNumberFor<T>) {
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

		/// Settle CDP has debit after system shutdown
		///
		/// The dispatch origin of this call must be _None_.
		///
		/// - `who`: CDP's owner.
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

		/// Update parameters related to risk management of CDP under specific
		/// collateral type
		///
		/// The dispatch origin of this call must be `UpdateOrigin`.
		///
		/// - `interest_rate_per_sec`: Interest rate per sec, `None` means do not update,
		/// - `liquidation_ratio`: liquidation ratio, `None` means do not update, `Some(None)` means
		///   update it to `None`.
		/// - `liquidation_penalty`: liquidation penalty, `None` means do not update, `Some(None)`
		///   means update it to `None`.
		/// - `required_collateral_ratio`: required collateral ratio, `None` means do not update,
		///   `Some(None)` means update it to `None`.
		/// - `maximum_total_debit_value`: maximum total debit value.
		#[pallet::call_index(2)]
		#[pallet::weight((<T as Config>::WeightInfo::set_collateral_params(), DispatchClass::Operational))]
		pub fn set_collateral_params(
			origin: OriginFor<T>,
			interest_rate_per_sec: ChangeOptionRate,
			liquidation_ratio: ChangeOptionRatio,
			liquidation_penalty: ChangeOptionRate,
			required_collateral_ratio: ChangeOptionRatio,
			maximum_total_debit_value: Change<pallet_loans::BalanceOf<T>>,
		) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;

			let mut collateral_params = Self::collateral_params();
			if let Change::NewValue(maybe_rate) = interest_rate_per_sec {
				match (collateral_params.interest_rate_per_sec.as_mut(), maybe_rate) {
					(Some(existing), Some(rate)) =>
						existing.try_set(rate).map_err(|_| Error::<T>::InvalidRate)?,
					(None, Some(rate)) => {
						let fractional_rate =
							FractionalRate::try_from(rate).map_err(|_| Error::<T>::InvalidRate)?;
						collateral_params.interest_rate_per_sec = Some(fractional_rate);
					},
					_ => collateral_params.interest_rate_per_sec = None,
				}
				Self::deposit_event(Event::InterestRatePerSecUpdated {
					new_interest_rate_per_sec: maybe_rate,
				});
			}
			if let Change::NewValue(update) = liquidation_ratio {
				collateral_params.liquidation_ratio = update;
				Self::deposit_event(Event::LiquidationRatioUpdated {
					new_liquidation_ratio: update,
				});
			}
			if let Change::NewValue(maybe_rate) = liquidation_penalty {
				match (collateral_params.liquidation_penalty.as_mut(), maybe_rate) {
					(Some(existing), Some(rate)) =>
						existing.try_set(rate).map_err(|_| Error::<T>::InvalidRate)?,
					(None, Some(rate)) => {
						let fractional_rate =
							FractionalRate::try_from(rate).map_err(|_| Error::<T>::InvalidRate)?;
						collateral_params.liquidation_penalty = Some(fractional_rate);
					},
					_ => collateral_params.liquidation_penalty = None,
				}
				Self::deposit_event(Event::LiquidationPenaltyUpdated {
					new_liquidation_penalty: maybe_rate,
				});
			}
			if let Change::NewValue(update) = required_collateral_ratio {
				collateral_params.required_collateral_ratio = update;
				Self::deposit_event(Event::RequiredCollateralRatioUpdated {
					new_required_collateral_ratio: update,
				});
			}
			if let Change::NewValue(val) = maximum_total_debit_value {
				collateral_params.maximum_total_debit_value = val;
				Self::deposit_event(Event::MaximumTotalDebitValueUpdated {
					new_total_debit_value: val,
				});
			}
			CollateralParams::<T>::put(collateral_params);
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
						Self::check_cdp_status(collateral, debit, stability_fee),
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
		if !T::EmergencyShutdown::is_shutdown() && !now_secs.is_zero() {
			let interval_secs = now_secs.saturating_sub(last_accumulation_secs);
			let mut touched_entries: u32 = 0;

			for (stability_fee, total_debits) in pallet_loans::TotalDebitByStabilityFee::<T>::iter()
			{
				if total_debits.is_zero() {
					continue;
				}

				let rate_to_accumulate = Self::compound_interest_rate(stability_fee, interval_secs);
				if rate_to_accumulate.is_zero() {
					continue;
				}

				let debit_exchange_rate = Self::get_debit_exchange_rate(stability_fee);
				let debit_exchange_rate_increment =
					debit_exchange_rate.saturating_mul(rate_to_accumulate);
				let issued_stable_coin_balance =
					debit_exchange_rate_increment.saturating_mul_int(total_debits);

				// issue stablecoin to surplus pool
				let res = <T as Config>::CDPTreasury::on_system_surplus(issued_stable_coin_balance);
				match res {
					Ok(_) => {
						let new_debit_exchange_rate =
							debit_exchange_rate.saturating_add(debit_exchange_rate_increment);
						DebitExchangeRate::<T>::insert(stability_fee, new_debit_exchange_rate);
						touched_entries = touched_entries.saturating_add(1);
					},
					Err(e) => {
						log::warn!(
							target: "cdp-engine",
							"on_system_surplus: failed to on system surplus {:?}: {:?}. This is unexpected but should be safe",
							issued_stable_coin_balance,
							e
						);
					},
				}
			}

			LastAccumulationSecs::<T>::put(now_secs);
			return touched_entries.saturating_add(1);
		}

		// update last accumulation timestamp
		LastAccumulationSecs::<T>::put(now_secs);
		0
	}

	fn submit_unsigned_liquidation_tx(who: T::AccountId) {
		let who = T::Lookup::unlookup(who);
		let call = Call::<T>::liquidate { who: who.clone() };
		let res = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into());
		if res.is_err() {
			log::info!(
				target: "cdp-engine offchain worker",
				"submit unsigned liquidation tx for \nCDP - AccountId {:?} \nfailed!",
				who,
			);
		}
	}

	fn submit_unsigned_settlement_tx(who: T::AccountId) {
		let who = T::Lookup::unlookup(who);
		let call = Call::<T>::settle { who: who.clone() };
		let res = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into());
		if res.is_err() {
			log::info!(
				target: "cdp-engine offchain worker",
				"submit unsigned settlement tx for \nCDP - AccountId {:?} \nfailed!",
				who,
			);
		}
	}

	fn _offchain_worker() -> Result<(), OffchainErr> {
		// check if we are a potential validator
		if !sp_io::offchain::is_validator() {
			return Err(OffchainErr::NotValidator);
		}

		// acquire offchain worker lock
		let lock_expiration = Duration::from_millis(LOCK_DURATION);
		let mut lock = StorageLock::<Time>::with_deadline(OFFCHAIN_WORKER_LOCK, lock_expiration);
		let mut guard = lock.try_lock().map_err(|_| OffchainErr::OffchainLock)?;
		let to_be_continue = StorageValueRef::persistent(OFFCHAIN_WORKER_DATA);

		// get to_be_continue record
		let start_key: Option<Vec<u8>> = if let Ok(Some(maybe_last_iterator_previous_key)) =
			to_be_continue.get::<Option<Vec<u8>>>()
		{
			maybe_last_iterator_previous_key
		} else {
			None
		};

		// get the max iterations config
		let max_iterations = StorageValueRef::persistent(OFFCHAIN_WORKER_MAX_ITERATIONS)
			.get::<u32>()
			.unwrap_or(Some(DEFAULT_MAX_ITERATIONS))
			.unwrap_or(DEFAULT_MAX_ITERATIONS);

		let currency_id = T::GetNativeCurrencyId::get();
		let is_shutdown = T::EmergencyShutdown::is_shutdown();

		// If start key is Some(value) continue iterating from that point in storage otherwise start
		// iterating from the beginning of <pallet_loans::Positions<T>>
		let mut map_iterator = match start_key.clone() {
			Some(key) => <pallet_loans::Positions<T>>::iter_from(key),
			None => <pallet_loans::Positions<T>>::iter(),
		};

		let mut finished = true;
		let mut iteration_count = 0;
		let iteration_start_time = sp_io::offchain::timestamp();

		#[allow(clippy::while_let_on_iterator)]
		while let Some((who, Position { collateral, debit, stability_fee })) = map_iterator.next() {
			let stability_fee = Rate::from_inner(stability_fee.into_inner());
			let stability_fee = match Self::get_effective_stability_fee(stability_fee) {
				Ok(fee) => fee,
				Err(e) => {
					log::debug!(
						target: "cdp-engine offchain worker",
						"skip position {:?} due to stability fee error: {:?}",
						who,
						e
					);
					continue;
				},
			};
			if !is_shutdown &&
				matches!(
					Self::check_cdp_status(collateral, debit, stability_fee),
					CDPStatus::Unsafe
				) {
				// liquidate unsafe CDPs before emergency shutdown occurs
				Self::submit_unsigned_liquidation_tx(who);
			} else if is_shutdown && !debit.is_zero() {
				// settle CDPs with debit after emergency shutdown occurs.
				Self::submit_unsigned_settlement_tx(who);
			}

			iteration_count += 1;
			if iteration_count == max_iterations {
				finished = false;
				break;
			}
			// extend offchain worker lock
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

		// if iteration for map storage finished, clear to be continue record
		// otherwise, update to be continue record
		if finished {
			to_be_continue.set(&Option::<Vec<u8>>::None);
		} else {
			to_be_continue.set(&Some(map_iterator.last_raw_key()));
		}

		// Consume the guard but **do not** unlock the underlying lock.
		guard.forget();

		Ok(())
	}

	pub fn check_cdp_status(
		collateral_amount: BalanceOf<T>,
		debit_amount: BalanceOf<T>,
		stability_fee: Rate,
	) -> CDPStatus {
		let currency_id = T::GetNativeCurrencyId::get();
		let stable_currency_id = T::GetStableCurrencyId::get();
		if let Some(feed_price) =
			T::PriceSource::get_relative_price(currency_id, stable_currency_id)
		{
			let stability_fee = match Self::get_effective_stability_fee(stability_fee) {
				Ok(fee) => fee,
				Err(e) => return CDPStatus::ChecksFailed(e),
			};
			let exchange_rate = Self::get_debit_exchange_rate(stability_fee);
			let collateral_ratio = Self::calculate_collateral_ratio(
				collateral_amount,
				debit_amount,
				feed_price,
				exchange_rate,
			);
			match Self::get_liquidation_ratio() {
				Ok(liquidation_ratio) =>
					if collateral_ratio < liquidation_ratio {
						CDPStatus::Unsafe
					} else {
						CDPStatus::Safe
					},
				Err(e) => CDPStatus::ChecksFailed(e),
			}
		} else {
			CDPStatus::ChecksFailed(Error::<T>::InvalidFeedPrice.into())
		}
	}

	pub fn maximum_total_debit_value() -> Result<BalanceOf<T>, DispatchError> {
		let params = Self::collateral_params();
		Ok(params.maximum_total_debit_value)
	}

	pub fn required_collateral_ratio() -> Result<Option<Ratio>, DispatchError> {
		let params = Self::collateral_params();
		Ok(params.required_collateral_ratio)
	}

	pub fn get_interest_rate_per_sec() -> Result<Rate, DispatchError> {
		let params = Self::collateral_params();
		params
			.interest_rate_per_sec
			.map(|v| v.into_inner())
			.ok_or_else(|| Error::<T>::InvalidRate.into())
	}

	pub fn get_effective_stability_fee(stability_fee: Rate) -> Result<Rate, DispatchError> {
		let current_stability_fee = Self::get_interest_rate_per_sec()?;
		Ok(sp_std::cmp::min(stability_fee, current_stability_fee))
	}

	pub fn compound_interest_rate(rate_per_sec: Rate, secs: u64) -> Rate {
		rate_per_sec
			.saturating_add(Rate::one())
			.saturating_pow(secs.unique_saturated_into())
			.saturating_sub(Rate::one())
	}

	pub fn get_liquidation_ratio() -> Result<Ratio, DispatchError> {
		let params = Self::collateral_params();
		Ok(params.liquidation_ratio.unwrap_or_else(T::DefaultLiquidationRatio::get))
	}

	pub fn get_liquidation_penalty() -> Result<Rate, DispatchError> {
		let params = Self::collateral_params();
		Ok(params
			.liquidation_penalty
			.map(|v| v.into_inner())
			.unwrap_or_else(|| T::DefaultLiquidationPenalty::get().into_inner()))
	}

	pub fn get_debit_exchange_rate(stability_fee: Rate) -> ExchangeRate {
		if let Some(exchange_rate) = DebitExchangeRate::<T>::get(stability_fee) {
			exchange_rate
		} else {
			let default_rate = T::DefaultDebitExchangeRate::get();
			DebitExchangeRate::<T>::insert(stability_fee, default_rate);
			default_rate
		}
	}

	pub fn convert_to_debit_value(
		debit_balance: BalanceOf<T>,
		stability_fee: Rate,
	) -> BalanceOf<T> {
		Self::get_debit_exchange_rate(stability_fee).saturating_mul_int(debit_balance)
	}

	pub fn try_convert_to_debit_balance(
		debit_value: BalanceOf<T>,
		stability_fee: Rate,
	) -> Option<BalanceOf<T>> {
		Self::get_debit_exchange_rate(stability_fee)
			.reciprocal()
			.map(|n| n.saturating_mul_int(debit_value))
	}

	fn average_debit_exchange_rate() -> ExchangeRate {
		let mut total_debit = BalanceOf::<T>::zero();
		let mut total_value = BalanceOf::<T>::zero();

		for (stability_fee, debit_balance) in pallet_loans::TotalDebitByStabilityFee::<T>::iter() {
			if debit_balance.is_zero() {
				continue;
			}
			let rate = Self::get_debit_exchange_rate(stability_fee);
			total_debit = total_debit.saturating_add(debit_balance);
			total_value = total_value.saturating_add(rate.saturating_mul_int(debit_balance));
		}

		if total_debit.is_zero() {
			T::DefaultDebitExchangeRate::get()
		} else {
			ExchangeRate::checked_from_rational(total_value, total_debit)
				.unwrap_or_else(T::DefaultDebitExchangeRate::get)
		}
	}

	pub fn calculate_collateral_ratio(
		collateral_balance: BalanceOf<T>,
		debit_balance: BalanceOf<T>,
		price: Price,
		exchange_rate: ExchangeRate,
	) -> Ratio {
		let locked_collateral_value = price.saturating_mul_int(collateral_balance);
		let debit_value = exchange_rate.saturating_mul_int(debit_balance);

		Ratio::checked_from_rational(locked_collateral_value, debit_value)
			.unwrap_or_else(Ratio::max_value)
	}

	pub fn adjust_position(
		who: &T::AccountId,
		collateral_adjustment: Amount,
		debit_adjustment: Amount,
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
		who: &T::AccountId,
		collateral_adjustment: Amount,
		debit_value_adjustment: Amount,
	) -> DispatchResult {
		let debit_value_adjustment_abs =
			<LoansOf<T>>::balance_try_from_amount_abs(debit_value_adjustment)?;
		let Position { debit, stability_fee, .. } = <LoansOf<T>>::positions(who);
		let position_stability_fee = Rate::from_inner(stability_fee.into_inner());
		let effective_stability_fee = Self::get_effective_stability_fee(position_stability_fee)?;

		if debit_value_adjustment.is_negative() {
			let debit_adjustment_abs = Self::try_convert_to_debit_balance(
				debit_value_adjustment_abs,
				effective_stability_fee,
			)
			.ok_or(Error::<T>::ConvertDebitBalanceFailed)?;
			let actual_adjustment_abs = debit.min(debit_adjustment_abs);
			let debit_adjustment = <LoansOf<T>>::amount_try_from_balance(actual_adjustment_abs)?;

			Self::adjust_position(
				who,
				collateral_adjustment,
				debit_adjustment.saturating_neg(),
				None,
			)?;
		} else {
			let new_stability_fee = Self::get_interest_rate_per_sec()?;
			let debit_adjustment_abs =
				Self::try_convert_to_debit_balance(debit_value_adjustment_abs, new_stability_fee)
					.ok_or(Error::<T>::ConvertDebitBalanceFailed)?;
			let debit_adjustment = <LoansOf<T>>::amount_try_from_balance(debit_adjustment_abs)?;

			Self::adjust_position(
				who,
				collateral_adjustment,
				debit_adjustment,
				Some(new_stability_fee),
			)?;
		}

		Ok(())
	}

	/// Generate new debit in advance, buy collateral and deposit it into CDP,
	/// and the collateral ratio will be reduced but CDP must still be at valid risk.
	#[transactional]
	pub fn expand_position_collateral(
		who: &T::AccountId,
		increase_debit_value: BalanceOf<T>,
		min_increase_collateral: BalanceOf<T>,
	) -> DispatchResult {
		let currency_id = T::GetNativeCurrencyId::get();
		let loans_module_account = <LoansOf<T>>::account_id();

		// issue stable coin in advance
		<T as Config>::CDPTreasury::issue_debit(&loans_module_account, increase_debit_value, true)?;

		// swap stable coin to collateral
		let limit = SwapLimit::ExactSupply(increase_debit_value, min_increase_collateral);
		let (_, increase_collateral) = T::Swap::swap_by_path(
			&loans_module_account,
			&[T::GetStableCurrencyId::get(), currency_id],
			limit,
		)?;

		// update CDP state
		let collateral_adjustment = <LoansOf<T>>::amount_try_from_balance(increase_collateral)?;
		let new_stability_fee = Self::get_interest_rate_per_sec()?;
		let increase_debit_balance =
			Self::try_convert_to_debit_balance(increase_debit_value, new_stability_fee)
				.ok_or(Error::<T>::ConvertDebitBalanceFailed)?;
		let debit_adjustment = <LoansOf<T>>::amount_try_from_balance(increase_debit_balance)?;
		Self::adjust_position(
			who,
			collateral_adjustment,
			debit_adjustment,
			Some(new_stability_fee),
		)?;

		let Position { collateral, debit, .. } = <LoansOf<T>>::positions(who);
		// check the CDP if is still at valid risk
		Self::check_position_valid(currency_id, collateral, debit, false)?;
		// debit cap check due to new issued stable coin
		let Position { debit: total_debit, .. } = <LoansOf<T>>::total_positions();
		Self::check_debit_cap(currency_id, total_debit)?;
		Ok(())
	}

	/// Sell the collateral locked in CDP to get stable coin to repay the debit,
	/// and the collateral ratio will be increased.
	#[transactional]
	pub fn shrink_position_debit(
		who: &T::AccountId,
		decrease_collateral: BalanceOf<T>,
		min_decrease_debit_value: BalanceOf<T>,
	) -> DispatchResult {
		let currency_id = T::GetNativeCurrencyId::get();
		let loans_module_account = <LoansOf<T>>::account_id();
		let stable_currency_id = T::GetStableCurrencyId::get();
		let Position { collateral, debit, stability_fee } = <LoansOf<T>>::positions(who);
		let position_stability_fee = Rate::from_inner(stability_fee.into_inner());
		let effective_stability_fee = Self::get_effective_stability_fee(position_stability_fee)?;

		// ensure collateral of CDP is enough
		ensure!(decrease_collateral <= collateral, Error::<T>::CollateralNotEnough);

		// swap collateral to stable coin
		let limit = SwapLimit::ExactSupply(decrease_collateral, min_decrease_debit_value);
		let (_, actual_stable_amount) = T::Swap::swap_by_path(
			&loans_module_account,
			&[currency_id, stable_currency_id],
			limit,
		)?;

		// update CDP state
		let collateral_adjustment =
			<LoansOf<T>>::amount_try_from_balance(decrease_collateral)?.saturating_neg();
		let previous_debit_value = Self::convert_to_debit_value(debit, effective_stability_fee);
		let (decrease_debit_value, decrease_debit_balance) = if actual_stable_amount >=
			previous_debit_value
		{
			// refund extra stable coin to the CDP owner
			<T as Config>::Currency::transfer(
				stable_currency_id,
				&loans_module_account,
				who,
				actual_stable_amount.saturating_sub(previous_debit_value),
				true,
			)?;

			(previous_debit_value, debit)
		} else {
			(
				actual_stable_amount,
				Self::try_convert_to_debit_balance(actual_stable_amount, effective_stability_fee)
					.ok_or(Error::<T>::ConvertDebitBalanceFailed)?,
			)
		};

		let debit_adjustment =
			<LoansOf<T>>::amount_try_from_balance(decrease_debit_balance)?.saturating_neg();
		Self::adjust_position(who, collateral_adjustment, debit_adjustment, None)?;

		// repay the debit of CDP
		<T as Config>::CDPTreasury::burn_debit(&loans_module_account, decrease_debit_value)?;

		// check the CDP if is still at valid risk.
		let Position { collateral: updated_collateral, debit: updated_debit, .. } =
			<LoansOf<T>>::positions(who);
		Self::check_position_valid(currency_id, updated_collateral, updated_debit, false)?;
		Ok(())
	}

	// settle cdp has debit when emergency shutdown
	pub fn settle_cdp_has_debit(who: T::AccountId) -> DispatchResult {
		let currency_id = T::GetNativeCurrencyId::get();
		let Position { collateral, debit, stability_fee } = <LoansOf<T>>::positions(&who);
		let stability_fee = Rate::from_inner(stability_fee.into_inner());
		let stability_fee = Self::get_effective_stability_fee(stability_fee)?;
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
		who: T::AccountId,
		max_collateral_amount: BalanceOf<T>,
	) -> DispatchResult {
		let currency_id = T::GetNativeCurrencyId::get();
		let Position { collateral, debit, stability_fee } = <LoansOf<T>>::positions(&who);
		let stability_fee = Rate::from_inner(stability_fee.into_inner());
		ensure!(!debit.is_zero(), Error::<T>::NoDebitValue);
		ensure!(
			matches!(Self::check_cdp_status(collateral, debit, stability_fee), CDPStatus::Safe),
			Error::<T>::MustBeSafe
		);
		let stability_fee = Self::get_effective_stability_fee(stability_fee)?;

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

	// liquidate unsafe cdp
	pub fn liquidate_unsafe_cdp(who: T::AccountId) -> Result<Weight, DispatchError> {
		let currency_id = T::GetNativeCurrencyId::get();
		let Position { collateral, debit, stability_fee } = <LoansOf<T>>::positions(&who);
		let stability_fee = Rate::from_inner(stability_fee.into_inner());

		// ensure the cdp is unsafe
		ensure!(
			matches!(Self::check_cdp_status(collateral, debit, stability_fee), CDPStatus::Unsafe),
			Error::<T>::MustBeUnsafe
		);
		let stability_fee = Self::get_effective_stability_fee(stability_fee)?;

		// confiscate all collateral and debit of unsafe cdp to cdp treasury
		<LoansOf<T>>::confiscate_collateral_and_debit(&who, collateral, debit)?;

		let bad_debt_value = Self::convert_to_debit_value(debit, stability_fee);
		let liquidation_penalty = Self::get_liquidation_penalty()?;
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

	pub fn handle_liquidated_collateral(
		who: &T::AccountId,
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

	fn account_id() -> T::AccountId {
		<T as Config>::PalletId::get().into_account_truncating()
	}
}

type LiquidateByPriority<T> = (LiquidateViaDex<T>, LiquidateViaAuction<T>);

pub struct LiquidateViaDex<T>(PhantomData<T>);
impl<T: Config> LiquidateCollateral<T::AccountId, CurrencyId, BalanceOf<T>> for LiquidateViaDex<T> {
	fn liquidate(
		who: &T::AccountId,
		_collateral_currency_id: CurrencyId,
		amount: BalanceOf<T>,
		target_stable_amount: BalanceOf<T>,
	) -> DispatchResult {
		let currency_id = T::GetNativeCurrencyId::get();
		// calculate the supply limit by slippage limit for the price of oracle,
		let max_supply_limit = Ratio::one()
			.saturating_sub(T::MaxSwapSlippageCompareToOracle::get())
			.reciprocal()
			.unwrap_or_else(Ratio::max_value)
			.saturating_mul_int(
				T::PriceSource::get_relative_price(T::GetStableCurrencyId::get(), currency_id)
					.expect("the oracle price should be available because liquidation are triggered by it.")
					.saturating_mul_int(target_stable_amount),
			);
		let collateral_supply = amount.min(max_supply_limit);

		let (actual_supply_collateral, actual_target_amount) =
			<T as Config>::CDPTreasury::swap_collateral_to_stable(
				SwapLimit::ExactTarget(collateral_supply, target_stable_amount),
				false,
			)?;

		let refund_collateral_amount = amount
			.checked_sub(&actual_supply_collateral)
			.expect("swap success means collateral >= actual_supply_collateral; qed");
		// refund remain collateral to CDP owner
		if !refund_collateral_amount.is_zero() {
			<T as Config>::CDPTreasury::withdraw_collateral(who, refund_collateral_amount)?;
		}

		// Note: for StableAsset, the swap of cdp treasury is always on `ExactSupply`
		// regardless of this swap_limit params. There will be excess stablecoins that
		// need to be returned to the `who` from cdp treasury account.
		if actual_target_amount > target_stable_amount {
			<T as Config>::CDPTreasury::withdraw_surplus(
				who,
				actual_target_amount.saturating_sub(target_stable_amount),
			)?;
		}

		Ok(())
	}
}

pub struct LiquidateViaAuction<T>(PhantomData<T>);
impl<T: Config> LiquidateCollateral<T::AccountId, CurrencyId, BalanceOf<T>>
	for LiquidateViaAuction<T>
{
	fn liquidate(
		who: &T::AccountId,
		_collateral_currency_id: CurrencyId,
		amount: BalanceOf<T>,
		target_stable_amount: BalanceOf<T>,
	) -> DispatchResult {
		<T as Config>::CDPTreasury::create_collateral_auctions(
			amount,
			target_stable_amount,
			who.clone(),
			true,
		)
		.map(|_| ())
	}
}

impl<T: Config> RiskManager<T::AccountId, CurrencyId, BalanceOf<T>, BalanceOf<T>> for Pallet<T> {
	fn get_debit_value(_currency_id: CurrencyId, debit_balance: BalanceOf<T>) -> BalanceOf<T> {
		Self::average_debit_exchange_rate().saturating_mul_int(debit_balance)
	}

	fn check_position_valid(
		_currency_id: CurrencyId,
		collateral_balance: BalanceOf<T>,
		debit_balance: BalanceOf<T>,
		check_required_ratio: bool,
	) -> DispatchResult {
		let currency_id = T::GetNativeCurrencyId::get();
		let exchange_rate = Self::average_debit_exchange_rate();
		if !debit_balance.is_zero() {
			let debit_value = exchange_rate.saturating_mul_int(debit_balance);
			let feed_price = <T as Config>::PriceSource::get_relative_price(
				currency_id,
				T::GetStableCurrencyId::get(),
			)
			.ok_or(Error::<T>::InvalidFeedPrice)?;
			let collateral_ratio = Self::calculate_collateral_ratio(
				collateral_balance,
				debit_balance,
				feed_price,
				exchange_rate,
			);

			if check_required_ratio {
				if let Some(required_collateral_ratio) = Self::required_collateral_ratio()? {
					ensure!(
						collateral_ratio >= required_collateral_ratio,
						Error::<T>::BelowRequiredCollateralRatio
					);
				}
			}

			let liquidation_ratio = Self::get_liquidation_ratio()?;
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

	fn check_debit_cap(
		_currency_id: CurrencyId,
		total_debit_balance: BalanceOf<T>,
	) -> DispatchResult {
		let hard_cap = Self::maximum_total_debit_value()?;
		let mut total_debit_value = BalanceOf::<T>::zero();
		for (stability_fee, debit_balance) in pallet_loans::TotalDebitByStabilityFee::<T>::iter() {
			if debit_balance.is_zero() {
				continue;
			}
			let exchange_rate = Self::get_debit_exchange_rate(stability_fee);
			total_debit_value =
				total_debit_value.saturating_add(exchange_rate.saturating_mul_int(debit_balance));
		}

		if total_debit_value.is_zero() && !total_debit_balance.is_zero() {
			let exchange_rate = Self::average_debit_exchange_rate();
			total_debit_value = exchange_rate.saturating_mul_int(total_debit_balance);
		}

		ensure!(total_debit_value <= hard_cap, Error::<T>::ExceedDebitValueHardCap);

		Ok(())
	}
}
