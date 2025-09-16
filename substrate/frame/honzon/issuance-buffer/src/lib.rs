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

//! # Issuance Buffer Module
//!
//! A governance-controlled pallet that provides a protocol-native backstop during liquidations.

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::unused_unit)]
#![allow(clippy::collapsible_if)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::{
        pallet_prelude::*,
        traits::{Currency, ReservableCurrency, Get}, PalletId,
    };
    use frame_system::pallet_prelude::*;
    use sp_runtime::{traits::{Zero, AccountIdConversion, Saturating}, Permill};
    use sp_arithmetic::FixedPointNumber;
    use pallet_traits::{LiquidationTarget, PriceProvider, CDPTreasury};
    use sp_std::result;

    type BalanceOf<T> = <<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_cdp_treasury::Config {
        /// The origin which can update parameters of the module.
        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Currency type for deposit/withdraw collateral assets to/from loans
        /// module
        type Currency: ReservableCurrency<Self::AccountId>;

        /// Price provider for collateral assets.
        type PriceProvider: PriceProvider<Self::CurrencyId>;

        /// The currency ID of the collateral managed by this buffer.
        #[pallet::constant]
        type CollateralCurrencyId: Get<Self::CurrencyId>;

        /// The currency ID of the stablecoin.
        #[pallet::constant]
        type StableCurrencyId: Get<Self::CurrencyId>;

        /// The pallet ID for the issuance buffer, used for deriving its account ID.
        #[pallet::constant]
        type PalletId: Get<PalletId>;

        /// The CDP treasury pallet.
        type CDPTreasury: CDPTreasury<Self::AccountId, Balance = BalanceOf<Self>, CurrencyId = Self::CurrencyId>;
    }

    #[pallet::storage]
    #[pallet::getter(fn discount)]
    pub type Discount<T> = StorageValue<_, Permill, ValueQuery>; // default: Permill::from_percent(100)

    #[pallet::storage]
    #[pallet::getter(fn issuance_quota)]
    pub type IssuanceQuota<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>; // max PDD debt

    #[pallet::storage]
    #[pallet::getter(fn issuance_used)]
    pub type IssuanceUsed<T: Config> = StorageValue<_, BalanceOf<T>, ValueQuery>; // current PDD debt

    #[pallet::event]
    #[pallet::generate_deposit(pub(crate) fn deposit_event)]
    pub enum Event<T: Config> {
        /// Buffer funded
        Funded { amount: BalanceOf<T> },
        /// Buffer defunded
        Defunded { amount: BalanceOf<T> },
        /// Discount rate updated
        DiscountUpdated { discount: Permill },
        /// Issuance quota updated
        IssuanceQuotaUpdated { quota: BalanceOf<T> },
    }

    #[pallet::error]
    pub enum Error<T> {
        /// Quota exceeded
        QuotaExceeded,
        /// Insufficient funds to withdraw
        InsufficientFunds,
        /// The collateral currency is not supported by this buffer.
        UnsupportedCollateral,
        /// An error occurred while trying to get the price from the oracle.
        OraclePriceError,
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Fund the buffer by locking DOT as collateral into the buffer CDP.
        #[pallet::call_index(0)]
        #[pallet::weight(T::DbWeight::get().writes(1))]
        pub fn fund(
            origin: OriginFor<T>,
            #[pallet::compact] amount: BalanceOf<T>,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;

            T::CDPTreasury::pay_surplus(amount)?;

            Self::deposit_event(Event::Funded { amount });
            Ok(())
        }

        /// Withdraw unlocked DOT from the buffer CDP (only collateral not required
        /// by LR/CR and not reserved for in-flight liquidations can be withdrawn).
        #[pallet::call_index(1)]
        #[pallet::weight(T::DbWeight::get().writes(1))]
        pub fn defund(
            origin: OriginFor<T>,
            #[pallet::compact] amount: BalanceOf<T>,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;

            T::CDPTreasury::refund_surplus(amount)?;

            Self::deposit_event(Event::Defunded { amount });
            Ok(())
        }

        /// Set the discount factor used to price bids vs oracle. For example,
        /// discount = 95% means bid = 0.95 * oracle_price (a 5% discount).
        #[pallet::call_index(2)]
        #[pallet::weight(T::DbWeight::get().writes(1))]
        pub fn set_discount(origin: OriginFor<T>, discount: Permill) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;

            Discount::<T>::put(discount);

            Self::deposit_event(Event::DiscountUpdated { discount });
            Ok(())
        }

        /// Set the maximum additional PDD the buffer may issue (as debt on its CDP).
        #[pallet::call_index(3)]
        #[pallet::weight(T::DbWeight::get().writes(1))]
        pub fn set_issuance_quota(
            origin: OriginFor<T>,
            #[pallet::compact] quota: BalanceOf<T>,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;

            IssuanceQuota::<T>::put(quota);

            Self::deposit_event(Event::IssuanceQuotaUpdated { quota });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn account_id() -> T::AccountId {
            <T as Config>::PalletId::get().into_account_truncating()
        }
    }

    impl<T: Config> LiquidationTarget<T::AccountId, T::CurrencyId, BalanceOf<T>> for Pallet<T> {
        fn liquidate(
            who: &T::AccountId,
            collateral_currency: T::CurrencyId,
            collateral_to_sell: BalanceOf<T>,
            debit_to_cover: BalanceOf<T>,
        ) -> result::Result<(BalanceOf<T>, BalanceOf<T>), DispatchError> {
            if collateral_currency != <T as Config>::CollateralCurrencyId::get() {
                return Err(Error::<T>::UnsupportedCollateral.into());
            }

            let price = T::PriceProvider::get_relative_price(collateral_currency, T::StableCurrencyId::get()).ok_or(Error::<T>::OraclePriceError)?;
            let discount = Self::discount();
            let discounted_price = price.saturating_mul(discount.into());

            let remaining_quota = Self::issuance_quota().saturating_sub(Self::issuance_used());
            if remaining_quota.is_zero() {
                return Ok((Zero::zero(), Zero::zero()));
            }

            // how much collateral can be bought with remaining quota
            let collateral_to_buy: BalanceOf<T> = if let Some(r) = discounted_price.reciprocal() {
                r.saturating_mul_int(remaining_quota)
            } else {
                Zero::zero()
            }.into();

            let actual_collateral_to_buy = sp_std::cmp::min(collateral_to_sell, collateral_to_buy);
            if actual_collateral_to_buy.is_zero() {
                return Ok((Zero::zero(), Zero::zero()));
            }

            let debit_value = discounted_price.saturating_mul_int(actual_collateral_to_buy);
            let actual_debit_to_cover = sp_std::cmp::min(debit_to_cover, debit_value);

            // recalculate collateral to buy based on actual debit to cover
            let actual_collateral_to_buy_final: BalanceOf<T> = if let Some(r) = discounted_price.reciprocal() {
                r.saturating_mul_int(actual_debit_to_cover)
            } else {
                Zero::zero()
            }.into();

            <T as Config>::Currency::transfer(who, &Self::account_id(), actual_collateral_to_buy_final, frame_support::traits::ExistenceRequirement::AllowDeath)?;

            T::CDPTreasury::deposit_collateral(&Self::account_id(), actual_collateral_to_buy_final)?;
            T::CDPTreasury::on_system_debit(actual_debit_to_cover)?;

            IssuanceUsed::<T>::mutate(|used| *used = used.saturating_add(actual_debit_to_cover));

            Ok((actual_collateral_to_buy_final, actual_debit_to_cover))
        }
    }
}

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;
