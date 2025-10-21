// Copyright (C) 2020-2025 Acala Foundation.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Benchmarking for issuance-buffer.

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as IssuanceBuffer;
use frame_benchmarking::{benchmarks, whitelisted_caller};
use frame_support::traits::Get;
use frame_system::RawOrigin;
use sp_std::prelude::*;

benchmarks! {
    fund {
        let caller: T::AccountId = whitelisted_caller();
        let amount: BalanceOf<T> = T::IssuanceQuota::get();

    }: _(RawOrigin::Root, amount)

    defund {
        let caller: T::AccountId = whitelisted_caller();
        let amount: BalanceOf<T> = T::IssuanceQuota::get();
        IssuanceBuffer::<T>::fund(RawOrigin::Root.into(), amount)?;

    }: _(RawOrigin::Root, amount)

    impl_benchmark_test_suite!(
        IssuanceBuffer,
        crate::mock::new_test_ext(),
        crate::mock::Test
    );
}
