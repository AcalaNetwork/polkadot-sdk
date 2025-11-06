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

//! Implementation of the VaultProvider trait for the CDP Engine pallet.
use crate::*;
use frame_support::{
	ensure,
	traits::{
		fungible::Mutate,
		fungibles::Mutate as FungiblesMutate,
		honzon::{Rate, VaultProvider},
		tokens::Preservation,
		Get,
	},
	transactional,
};
use pallet_loans::BalanceOf;
use sp_runtime::{
	traits::{AccountIdConversion, Zero},
	DispatchError, DispatchResult,
};

fn vault_account_id<T: Config>(vault_id: T::VaultId) -> T::AccountId {
	<T as crate::Config>::PalletId::get().into_sub_account_truncating(vault_id)
}

impl<T: Config> VaultProvider<T::AccountId, BalanceOf<T>, CurrencyIdOf<T>, T::VaultId>
	for Pallet<T>
{
	type AssetId = CurrencyIdOf<T>;

	fn get_position(vault_id: &T::VaultId) -> Result<(BalanceOf<T>, BalanceOf<T>), DispatchError> {
		let vault_account = vault_account_id::<T>(*vault_id);
		let position = <LoansOf<T>>::positions(&vault_account);
		let stability_fee = Rate::from_inner(position.stability_fee.into_inner());
		let effective_stability_fee =
			Self::get_effective_stability_fee(stability_fee, &vault_account);
		let debit_value = Self::convert_to_debit_value(position.debit, effective_stability_fee);
		Ok((position.collateral, debit_value))
	}

	fn withdraw_all_collateral(vault_id: &T::VaultId, to: &T::AccountId) -> DispatchResult {
		let vault_account = vault_account_id::<T>(*vault_id);
		let position = <LoansOf<T>>::positions(&vault_account);
		Self::withdraw_collateral(vault_id, to, position.collateral)
	}

	fn set_stability_fee(vault_id: &T::VaultId, stability_fee: Option<Rate>) -> DispatchResult {
		if let Some(fee) = stability_fee {
			StabilityFeeOverrides::<T>::insert(vault_id, fee);
		} else {
			StabilityFeeOverrides::<T>::remove(vault_id);
		}
		Ok(())
	}

	#[transactional]
	fn deposit_collateral(
		vault_id: &T::VaultId,
		from: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		let vault_account = vault_account_id::<T>(*vault_id);
		VaultIdByAccountId::<T>::insert(&vault_account, *vault_id);

		// Transfer collateral from the user to the vault's account.
		<T as Config>::Currency::transfer(from, &vault_account, amount, Preservation::Preserve)?;

		// Increase the collateral in the vault's position.
		let collateral_adjustment = pallet_loans::BalanceAdjustment::increase(amount);
		let debit_adjustment = pallet_loans::BalanceAdjustment::increase(BalanceOf::<T>::zero());
		<LoansOf<T>>::adjust_position(
			&vault_account,
			collateral_adjustment,
			debit_adjustment,
			None,
		)?;

		Ok(())
	}

	#[transactional]
	fn withdraw_collateral(
		vault_id: &T::VaultId,
		to: &T::AccountId,
		amount: BalanceOf<T>,
	) -> DispatchResult {
		let vault_account = vault_account_id::<T>(*vault_id);

		// Decrease the collateral in the vault's position.
		let collateral_adjustment = pallet_loans::BalanceAdjustment::decrease(amount);
		let debit_adjustment = pallet_loans::BalanceAdjustment::increase(BalanceOf::<T>::zero());
		<LoansOf<T>>::adjust_position(
			&vault_account,
			collateral_adjustment,
			debit_adjustment,
			None,
		)?;

		// Transfer the released collateral from the vault's account back to the user.
		<T as Config>::Currency::transfer(&vault_account, to, amount, Preservation::Preserve)?;

		Ok(())
	}

	#[transactional]
	fn mint(vault_id: &T::VaultId, to: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
		let vault_account = vault_account_id::<T>(*vault_id);
		VaultIdByAccountId::<T>::insert(&vault_account, *vault_id);
		let stable_currency_id = T::GetStableCurrencyId::get();

		// Convert the stablecoin amount to the equivalent debit amount.
		let debit_amount = Self::convert_to_debit_balance(amount, Self::interest_rate_per_sec());

		// Increase the debit in the vault's position. This will mint stablecoin to the vault's
		// account.
		let debit_adjustment = pallet_loans::BalanceAdjustment::increase(debit_amount);
		let collateral_adjustment =
			pallet_loans::BalanceAdjustment::increase(BalanceOf::<T>::zero());
		<LoansOf<T>>::adjust_position(
			&vault_account,
			collateral_adjustment,
			debit_adjustment,
			Some(Self::interest_rate_per_sec()),
		)?;

		// Transfer the newly minted stablecoin from the vault's account to the user.
		<T as Config>::Tokens::transfer(
			stable_currency_id,
			&vault_account,
			to,
			amount,
			Preservation::Preserve,
		)?;

		Ok(())
	}

	#[transactional]
	fn repay(vault_id: &T::VaultId, from: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
		let vault_account = vault_account_id::<T>(*vault_id);
		let stable_currency_id = T::GetStableCurrencyId::get();
		let position = <LoansOf<T>>::positions(&vault_account);
		let stability_fee = Rate::from_inner(position.stability_fee.into_inner());

		// Transfer stablecoin from the user to the vault's account for repayment.
		<T as Config>::Tokens::transfer(
			stable_currency_id,
			from,
			&vault_account,
			amount,
			Preservation::Preserve,
		)?;

		// Convert the stablecoin amount to the equivalent debit amount.
		let debit_amount = Self::convert_to_debit_balance(
			amount,
			Self::get_effective_stability_fee(stability_fee, &vault_account),
		);

		// Decrease the debit in the vault's position. This will burn the stablecoin from the
		// vault's account.
		let debit_adjustment = pallet_loans::BalanceAdjustment::decrease(debit_amount);
		let collateral_adjustment =
			pallet_loans::BalanceAdjustment::increase(BalanceOf::<T>::zero());
		<LoansOf<T>>::adjust_position(
			&vault_account,
			collateral_adjustment,
			debit_adjustment,
			None,
		)?;

		Ok(())
	}

	fn close_vault(vault_id: &T::VaultId) -> DispatchResult {
		let vault_account = vault_account_id::<T>(*vault_id);
		let position = <LoansOf<T>>::positions(&vault_account);
		ensure!(position.collateral.is_zero() && position.debit.is_zero(), "Vault is not empty");
		VaultIdByAccountId::<T>::remove(&vault_account);
		Ok(())
	}

	fn has_debt(vault_id: &T::VaultId) -> Result<bool, DispatchError> {
		let vault_account = vault_account_id::<T>(*vault_id);
		let position = <LoansOf<T>>::positions(&vault_account);
		Ok(!position.debit.is_zero())
	}
}
