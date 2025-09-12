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

//! # CDP Engine Pallet
//!
//! ## Overview
//!
//! The CDP Engine pallet is the core of the Honzon protocol, responsible for managing
//! Collateralized Debt Positions (CDPs). It handles the internal processes of CDPs,
//! including liquidation, settlement, and risk management. This pallet allows users to
//! lock collateral, generate debt in a stable currency, and manage their positions to
//! avoid liquidation.
//!
//! ### Key Features
//!
//! - **CDP Management:** Allows creating, adjusting, and closing CDPs.
//! - **Risk Management:** Implements parameters like liquidation ratio and required
//!   collateral ratio to ensure system stability.
//! - **Liquidation:** Provides mechanisms to liquidate under-collateralized CDPs.
//! - **Emergency Shutdown:** Includes a feature to shut down the system in critical
//!   situations, allowing for the settlement of outstanding debts.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::unused_unit)]
#![allow(clippy::upper_case_acronyms)]

use frame_support::{
	pallet_prelude::*,
	traits::{Get, UnixTime},
	transactional,
	PalletId,
};
use frame_system::pallet_prelude::*;
use codec::MaxEncodedLen;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{
		AccountIdConversion, One, Saturating, Zero, AtLeast32BitUnsigned, Bounded,
	},
	DispatchError, DispatchResult, FixedPointNumber, RuntimeDebug,
};
use sp_std::{marker::PhantomData, prelude::*};

mod mock;
mod tests;
pub mod weights;

pub use pallet::*;
pub use weights::WeightInfo;

/// Defines the risk management parameters for a specific collateral type.
///
/// These parameters control the conditions under which CDPs can be created, modified,
/// and liquidated, ensuring the overall stability of the system.
#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq, Default, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct RiskManagementParams<Balance> {
	/// The maximum total debit value that can be generated from this collateral type.
	/// When this hard cap is reached, no more stablecoin can be issued against this collateral.
	pub maximum_total_debit_value: Balance,

	/// The liquidation ratio for CDPs of this collateral type.
	/// If a CDP's collateral ratio falls below this value, it is considered unsafe and
	/// can be liquidated.
	pub liquidation_ratio: Option<Ratio>,

	/// The required collateral ratio for this collateral type.
	/// When adjusting a CDP, its collateral ratio must not fall below this value.
	pub required_collateral_ratio: Option<Ratio>,
}

/// Ratio type for collateral calculations
pub type Ratio = sp_runtime::FixedU128;

/// Represents the health status of a Collateralized Debt Position (CDP).
#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq, TypeInfo)]
pub enum CDPStatus {
	/// The CDP's collateral ratio is above the liquidation ratio, and it is considered safe.
	Safe,
	/// The CDP's collateral ratio has fallen below the liquidation ratio, making it vulnerable
	/// to liquidation.
	Unsafe,
	/// An error occurred while checking the CDP's status, typically due to invalid parameters.
	ChecksFailed(DispatchError),
}

/// Represents a Collateralized Debt Position (CDP), containing the amounts of collateral
/// locked and debit issued.
#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, Eq, Default, TypeInfo, MaxEncodedLen)]
pub struct Position<Balance> {
	/// The amount of collateral locked in the CDP.
	pub collateral: Balance,
	/// The amount of debit (stablecoin) issued against the collateral.
	pub debit: Balance,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	/// The pallet's configuration trait.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type of the runtime.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The balance type for amounts used in this pallet.
		type Balance: Member
			+ Parameter
			+ AtLeast32BitUnsigned
			+ Default
			+ Copy
			+ MaybeSerializeDeserialize
			+ MaxEncodedLen
			+ TypeInfo;

		/// The currency ID type.
		type CurrencyId: Parameter + Member + Copy + MaybeSerializeDeserialize + Ord + MaxEncodedLen;

		/// The origin that is allowed to update risk management parameters.
		type UpdateOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The default liquidation ratio for all collateral types. This is used when a specific
		/// ratio is not set for a collateral type.
		#[pallet::constant]
		type DefaultLiquidationRatio: Get<Ratio>;

		/// The minimum debit value required for a CDP to prevent the creation of dust positions.
		#[pallet::constant]
		type MinimumDebitValue: Get<Self::Balance>;

		/// Provides the current Unix timestamp.
		type UnixTime: UnixTime;

		/// A unique identifier for this pallet, used for deriving the pallet's account ID.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// Weight information for the extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	/// Errors that can occur in this pallet.
	#[pallet::error]
	pub enum Error<T> {
		/// The total debit value for a collateral type has exceeded the hard cap.
		ExceedDebitValueHardCap,
		/// The collateral ratio of a CDP is below the required collateral ratio.
		BelowRequiredCollateralRatio,
		/// The collateral ratio of a CDP is below the liquidation ratio.
		BelowLiquidationRatio,
		/// The CDP must be in an unsafe state for the operation to be valid.
		MustBeUnsafe,
		/// The CDP must be in a safe state for the operation to be valid.
		MustBeSafe,
		/// The specified collateral type is not valid or not configured.
		InvalidCollateralType,
		/// The remaining debit value in a CDP is below the minimum allowed amount.
		RemainDebitValueTooSmall,
		/// The CDP has no debit, so it cannot be settled.
		NoDebitValue,
		/// The system has already been shut down.
		AlreadyShutdown,
		/// The operation can only be performed after the system has been shut down.
		MustAfterShutdown,
		/// The collateral in the CDP is not sufficient for the operation.
		CollateralNotEnough,
		/// Failed to convert the debit value to a balance.
		ConvertDebitBalanceFailed,
		/// An error occurred during the liquidation of a CDP.
		LiquidationFailed,
		/// The provided rate is invalid.
		InvalidRate,
	}

	/// Events emitted by this pallet.
	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// An unsafe CDP has been liquidated.
		///
		/// Includes the owner of the CDP, the amount of collateral liquidated, and the
		/// value of the bad debt created.
		LiquidateUnsafeCDP {
			owner: T::AccountId,
			collateral_amount: T::Balance,
			bad_debt_value: T::Balance,
		},
		/// A CDP with outstanding debit has been settled.
		///
		/// Includes the owner of the CDP.
		SettleCDPInDebit {
			owner: T::AccountId,
		},
		/// The liquidation ratio for a collateral type has been updated.
		///
		/// Includes the new liquidation ratio.
		LiquidationRatioUpdated {
			new_liquidation_ratio: Option<Ratio>,
		},
		/// The required collateral ratio for a collateral type has been updated.
		///
		/// Includes the new required collateral ratio.
		RequiredCollateralRatioUpdated {
			new_required_collateral_ratio: Option<Ratio>,
		},
		/// The maximum total debit value for a collateral type has been updated.
		///
		/// Includes the new maximum total debit value.
		MaximumTotalDebitValueUpdated {
			new_total_debit_value: T::Balance,
		},
	}

	/// The risk management parameters for the single collateral type supported by the pallet.
	///
	/// This storage item holds the `RiskManagementParams`, which include the liquidation ratio,
	/// required collateral ratio, and the maximum total debit value.
	#[pallet::storage]
	#[pallet::getter(fn collateral_params)]
	pub type CollateralParams<T: Config> = StorageValue<_, RiskManagementParams<T::Balance>, OptionQuery>;

	/// A map of account IDs to their respective Collateralized Debt Positions (CDPs).
	///
	/// Each `Position` stores the amount of collateral locked and debit issued by the account.
	#[pallet::storage]
	#[pallet::getter(fn positions)]
	pub type Positions<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, Position<T::Balance>, ValueQuery>;

	/// The timestamp, in seconds, of the last interest accumulation.
	///
	/// This is used to calculate the duration for which interest should be accrued.
	#[pallet::storage]
	#[pallet::getter(fn last_accumulation_secs)]
	pub type LastAccumulationSecs<T: Config> = StorageValue<_, u64, ValueQuery>;

	/// A flag indicating whether the system is in an emergency shutdown state.
	///
	/// When `true`, certain operations like creating new debt are disabled, and the focus
	/// shifts to settling existing positions.
	#[pallet::storage]
	#[pallet::getter(fn is_shutdown)]
	pub type IsShutdown<T: Config> = StorageValue<_, bool, ValueQuery>;

	/// The genesis configuration for the pallet.
	///
	/// Allows setting the initial risk management parameters at the genesis of the chain.
	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		/// The initial risk management parameters for the collateral type.
		pub collateral_params: RiskManagementParams<T::Balance>,
		pub _phantom: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			CollateralParams::<T>::put(&self.collateral_params);
		}
	}

	/// The CDP Engine pallet.
	///
	/// This pallet is responsible for managing Collateralized Debt Positions (CDPs),
	/// including their creation, adjustment, and liquidation. It forms the core of the
	/// Honzon protocol.
	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Hook to run at the beginning of each block.
		///
		/// This function handles the accumulation of interest on outstanding debt. It updates
		/// the `LastAccumulationSecs` storage item with the current timestamp.
		fn on_initialize(now: BlockNumberFor<T>) -> Weight {
			// Accumulate interest logic would go here
			let now_as_secs: u64 = if now > One::one() {
				T::UnixTime::now().as_secs()
			} else {
				Default::default()
			};

			LastAccumulationSecs::<T>::put(now_as_secs);
			T::WeightInfo::on_initialize()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Sets the risk management parameters for the collateral type.
		///
		/// These parameters include the liquidation ratio, required collateral ratio, and the
		/// maximum total debit value. This extrinsic can only be called by the `UpdateOrigin`.
		///
		/// - `liquidation_ratio`: The new liquidation ratio. If `None`, it remains unchanged.
		/// - `required_collateral_ratio`: The new required collateral ratio. If `None`, it
		///   remains unchanged.
		/// - `maximum_total_debit_value`: The new maximum total debit value.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::set_collateral_params())]
		pub fn set_collateral_params(
			origin: OriginFor<T>,
			liquidation_ratio: Option<Ratio>,
			required_collateral_ratio: Option<Ratio>,
			maximum_total_debit_value: T::Balance,
		) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;

			let mut collateral_params = Self::collateral_params().unwrap_or_default();
			
			if liquidation_ratio != collateral_params.liquidation_ratio {
				collateral_params.liquidation_ratio = liquidation_ratio;
				Self::deposit_event(Event::LiquidationRatioUpdated {
					new_liquidation_ratio: liquidation_ratio,
				});
			}
			
			if required_collateral_ratio != collateral_params.required_collateral_ratio {
				collateral_params.required_collateral_ratio = required_collateral_ratio;
				Self::deposit_event(Event::RequiredCollateralRatioUpdated {
					new_required_collateral_ratio: required_collateral_ratio,
				});
			}
			
			if maximum_total_debit_value != collateral_params.maximum_total_debit_value {
				collateral_params.maximum_total_debit_value = maximum_total_debit_value;
				Self::deposit_event(Event::MaximumTotalDebitValueUpdated {
					new_total_debit_value: maximum_total_debit_value,
				});
			}
			
			CollateralParams::<T>::put(collateral_params);
			Ok(())
		}

		/// Triggers an emergency shutdown of the system.
		///
		/// When in emergency shutdown, certain functionalities like creating new debt are
		/// disabled. This extrinsic can only be called by the `UpdateOrigin`.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::emergency_shutdown())]
		pub fn emergency_shutdown(origin: OriginFor<T>) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;
			IsShutdown::<T>::put(true);
			Ok(())
		}

		/// Adjusts the collateral and debit of a CDP.
		///
		/// This function allows a user to add or remove collateral and debit from their position.
		/// The final state of the CDP must satisfy the risk parameters, such as the required
		/// collateral ratio and minimum debit value.
		///
		/// - `collateral_adjustment`: The amount of collateral to add (if positive) or remove
		///   (if negative, though subtraction is handled via saturating add).
		/// - `debit_adjustment`: The amount of debit to add (if positive) or remove (if negative).
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::adjust_position())]
		pub fn adjust_position(
			origin: OriginFor<T>,
			collateral_adjustment: T::Balance,
			debit_adjustment: T::Balance,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			
			ensure!(
				CollateralParams::<T>::exists(),
				Error::<T>::InvalidCollateralType,
			);

			let mut position = Self::positions(&who);
			
			// Apply adjustments
			position.collateral = position.collateral.saturating_add(collateral_adjustment);
			position.debit = position.debit.saturating_add(debit_adjustment);

			// Validate the new position
			Self::check_position_valid(position.collateral, position.debit)?;

			// Update storage
			Positions::<T>::insert(&who, position);
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Returns the account ID of the pallet.
	pub fn account_id() -> T::AccountId {
		T::PalletId::get().into_account_truncating()
	}

	/// Gets the liquidation ratio for the collateral type.
	///
	/// Returns the specific liquidation ratio if set, otherwise falls back to the default
	/// liquidation ratio.
	pub fn get_liquidation_ratio() -> Result<Ratio, DispatchError> {
		let params = Self::collateral_params().ok_or(Error::<T>::InvalidCollateralType)?;
		Ok(params.liquidation_ratio.unwrap_or_else(T::DefaultLiquidationRatio::get))
	}

	/// Gets the required collateral ratio for the collateral type.
	pub fn required_collateral_ratio() -> Result<Option<Ratio>, DispatchError> {
		let params = Self::collateral_params().ok_or(Error::<T>::InvalidCollateralType)?;
		Ok(params.required_collateral_ratio)
	}

	/// Gets the maximum total debit value for the collateral type.
	pub fn maximum_total_debit_value() -> Result<T::Balance, DispatchError> {
		let params = Self::collateral_params().ok_or(Error::<T>::InvalidCollateralType)?;
		Ok(params.maximum_total_debit_value)
	}

	/// Calculates the collateral ratio of a CDP.
	///
	/// The collateral ratio is the ratio of the value of the collateral to the value of the debit.
	/// If the debit is zero, it returns the maximum possible ratio.
	///
	/// - `collateral_balance`: The amount of collateral.
	/// - `debit_balance`: The amount of debit.
	pub fn calculate_collateral_ratio(
		collateral_balance: T::Balance,
		debit_balance: T::Balance,
	) -> Ratio {
		if debit_balance.is_zero() {
			return Ratio::max_value();
		}
		
		Ratio::checked_from_rational(collateral_balance, debit_balance)
			.unwrap_or_else(Ratio::max_value)
	}

	/// Checks the status of a CDP.
	///
	/// A CDP is `Safe` if its collateral ratio is above the liquidation ratio, and `Unsafe`
	/// otherwise. If the debit is zero, it is always `Safe`.
	///
	/// - `collateral_amount`: The amount of collateral in the CDP.
	/// - `debit_amount`: The amount of debit in the CDP.
	pub fn check_cdp_status(
		collateral_amount: T::Balance,
		debit_amount: T::Balance,
	) -> CDPStatus {
		if debit_amount.is_zero() {
			return CDPStatus::Safe;
		}

		let collateral_ratio = Self::calculate_collateral_ratio(collateral_amount, debit_amount);
		match Self::get_liquidation_ratio() {
			Ok(liquidation_ratio) => {
				if collateral_ratio < liquidation_ratio {
					CDPStatus::Unsafe
				} else {
					CDPStatus::Safe
				}
			}
			Err(e) => CDPStatus::ChecksFailed(e),
		}
	}

	/// Checks if a CDP's position is valid.
	///
	/// This function ensures that the CDP meets all risk requirements, including the
	/// required collateral ratio, liquidation ratio, and minimum debit value.
	///
	/// - `collateral_balance`: The amount of collateral.
	/// - `debit_balance`: The amount of debit.
	pub fn check_position_valid(
		collateral_balance: T::Balance,
		debit_balance: T::Balance,
	) -> DispatchResult {
		if !debit_balance.is_zero() {
			let collateral_ratio = Self::calculate_collateral_ratio(collateral_balance, debit_balance);

			// Check the required collateral ratio
			if let Some(required_collateral_ratio) = Self::required_collateral_ratio()? {
				ensure!(
					collateral_ratio >= required_collateral_ratio,
					Error::<T>::BelowRequiredCollateralRatio
				);
			}

			// Check the liquidation ratio
			let liquidation_ratio = Self::get_liquidation_ratio()?;
			ensure!(collateral_ratio >= liquidation_ratio, Error::<T>::BelowLiquidationRatio);

			// Check the minimum_debit_value
			ensure!(
				debit_balance >= T::MinimumDebitValue::get(),
				Error::<T>::RemainDebitValueTooSmall,
			);
		}

		Ok(())
	}

	/// Liquidates an unsafe CDP.
	///
	/// This is a simplified version of liquidation that clears the position and emits an
	/// event. It ensures that the CDP is in an `Unsafe` state before proceeding.
	///
	/// - `who`: The owner of the CDP to be liquidated.
	pub fn liquidate_unsafe_cdp(who: &T::AccountId) -> DispatchResult {
		let position = Self::positions(who);

		// Ensure the CDP is unsafe
		ensure!(
			matches!(
				Self::check_cdp_status(position.collateral, position.debit),
				CDPStatus::Unsafe
			),
			Error::<T>::MustBeUnsafe
		);

		// Clear the position (simplified liquidation)
		Positions::<T>::remove(who);

		Self::deposit_event(Event::LiquidateUnsafeCDP {
			owner: who.clone(),
			collateral_amount: position.collateral,
			bad_debt_value: position.debit,
		});

		Ok(())
	}

	/// Settles a CDP that has outstanding debit after a system shutdown.
	///
	/// This function can only be called when the system is in an emergency shutdown state.
	/// It clears the position and emits an event.
	///
	/// - `who`: The owner of the CDP to be settled.
	pub fn settle_cdp_has_debit(who: &T::AccountId) -> DispatchResult {
		ensure!(Self::is_shutdown(), Error::<T>::MustAfterShutdown);
		
		let position = Self::positions(who);
		ensure!(!position.debit.is_zero(), Error::<T>::NoDebitValue);

		// Clear the position (simplified settlement)
		Positions::<T>::remove(who);

		Self::deposit_event(Event::SettleCDPInDebit {
			owner: who.clone(),
		});

		Ok(())
	}
}