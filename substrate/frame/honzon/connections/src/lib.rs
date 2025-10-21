#![cfg_attr(not(feature = "std"), no_std)]

/// The `pallet-connections` pallet provides a standardized way for users to interact with various
/// DeFi protocols, referred to as "liquidity destinations." It acts as a bridge, allowing users
/// to open "connections" that are backed by vaults. These connections enable users to deposit
/// collateral, mint stablecoins, and then use those stablecoins in other DeFi applications, all
/// while their collateral remains managed by the vault system.
///
/// Key features of this pallet include:
/// - Opening and closing connections to liquidity destinations.
/// - Depositing and withdrawing collateral.
/// - Minting stablecoins against collateral.
/// - Initiating and managing withdrawals of stablecoins from liquidity destinations.
/// - Overriding stability fees for specific destinations through governance.

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;
#[cfg(feature = "runtime-benchmarks")]
pub use benchmarking::BenchmarkHelper;
pub use pallet::*;

use frame_support::traits::{
	fungibles::{Inspect, Mutate},
	honzon::{Connection, ConnectionStatus, LiquidityRouter, VaultProvider},
};

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		dispatch::DispatchResult,
		pallet_prelude::*,
		traits::{honzon::Rate, EnsureOrigin, Currency, ExistenceRequirement},
		PalletId,
	};
	use frame_system::pallet_prelude::*;
	use sp_runtime::traits::{AccountIdConversion, BlockNumberProvider};
	use sp_runtime::traits::{AtLeast32BitUnsigned, CheckedAdd, One, Saturating};

	pub type DestinationIdOf<T> = <<T as Config>::LiquidityRouter as LiquidityRouter<
		<T as frame_system::Config>::AccountId,
		<T as Config>::Balance,
	>>::DestinationId;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Currency: Currency<Self::AccountId, Balance = Self::Balance>;
		type Balance: Member + Parameter + AtLeast32BitUnsigned + Default + Copy + MaxEncodedLen;
		type BlockNumber: Member + Parameter + AtLeast32BitUnsigned + Default + Copy + MaxEncodedLen;
		type CurrencyId: Member
			+ Parameter
			+ Copy
			+ sp_runtime::traits::MaybeSerializeDeserialize
			+ Ord
			+ MaxEncodedLen;
		/// The provider for vault management logic.
		type VaultProvider: VaultProvider<
			Self::AccountId,
			Self::Balance,
			Self::CurrencyId,
			Self::ConnectionId,
		>;
		/// The router for liquidity destination logic.
		type LiquidityRouter: LiquidityRouter<Self::AccountId, Self::Balance>;
		/// The origin that can add new liquidity destinations. This is typically a governance body.
		type GovernanceOrigin: EnsureOrigin<Self::RuntimeOrigin>;
		/// The unique identifier for a connection.
		type ConnectionId: Member
			+ Parameter
			+ AtLeast32BitUnsigned
			+ Default
			+ Copy
			+ MaxEncodedLen;
		/// The provider for the current block number.
		type BlockNumberProvider: BlockNumberProvider<BlockNumber = Self::BlockNumber>;

		/// The currency ID for the stablecoin. This is the currency that is minted against the
		/// collateral in the vaults.
		#[pallet::constant]
		type StableCurrencyId: Get<Self::CurrencyId>;

		type Fungibles: Mutate<Self::AccountId, AssetId = Self::CurrencyId, Balance = Self::Balance>;


		/// The pallet id for this pallet. This is used to generate the pallet's account ID.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// Helper for preparing benchmarking state.
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkHelper: crate::BenchmarkHelper<
			Self::AccountId,
			Self::Balance,
			Self::ConnectionId,
			DestinationIdOf<Self>,
			Self::BlockNumber,
		>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new connection was opened.
		ConnectionOpened {
			/// The ID of the newly opened connection.
			connection_id: T::ConnectionId,
			/// The owner of the connection.
			owner: T::AccountId,
			/// The destination of the connection.
			destination_id:
				<T::LiquidityRouter as LiquidityRouter<T::AccountId, T::Balance>>::DestinationId,
			/// The amount of collateral deposited.
			collateral_deposited: T::Balance,
			/// The amount of stablecoins minted.
			stables_minted: T::Balance,
		},
		/// A withdrawal from a liquidity destination was initiated.
		WithdrawalInitiated {
			/// The ID of the connection.
			connection_id: T::ConnectionId,
			/// The amount of stablecoins to be withdrawn.
			stables_amount: T::Balance,
		},
		/// A withdrawal was successfully completed.
		WithdrawalCompleted {
			/// The ID of the connection.
			connection_id: T::ConnectionId,
			/// The amount of stablecoins repaid to the vault.
			stables_repaid: T::Balance,
		},
		/// Collateral was withdrawn from a connection.
		CollateralWithdrawn {
			/// The ID of the connection.
			connection_id: T::ConnectionId,
			/// The amount of collateral withdrawn.
			amount: T::Balance,
		},
		/// A connection was closed.
		ConnectionClosed {
			/// The ID of the closed connection.
			connection_id: T::ConnectionId,
		},
		/// The stability fee for a destination was overridden.
		StabilityFeeOverrideSet {
			/// The ID of the destination.
			destination_id: DestinationIdOf<T>,
			/// The new stability fee value, or `None` to remove the override.
			new_value: Option<Rate>,
		},
		/// Liquidity was injected into a destination.
		LiquidityInjected {
			/// The destination account that received the liquidity.
			destination: T::AccountId,
			/// The amount of stablecoins injected.
			amount: T::Balance,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The specified connection does not exist.
		ConnectionNotFound,
		/// The caller is not the owner of the connection.
		NotOwner,
		/// The connection is not in the expected state for the requested operation.
		InvalidStatus,
		/// The specified liquidity destination was not found.
		DestinationNotFound,
		/// The underlying vault could not be created.
		VaultCreationFailed,
		/// The user has not deposited enough collateral.
		InsufficientCollateral,
		/// Minting stablecoins failed.
		MintFailed,
		/// Depositing funds into the liquidity destination failed.
		DepositFailed,
		/// Withdrawing funds from the liquidity destination failed.
		WithdrawalFailed,
		/// The origin of the call is not authorized to perform the operation.
		BadOrigin,
		/// The amount provided does not match the expected amount.
		AmountMismatch,
		/// Repaying stablecoins to the vault failed.
		RepayFailed,
		/// Withdrawing collateral from the vault failed.
		WithdrawCollateralFailed,
		/// Could not retrieve the vault's position.
		VaultPosition,
		/// The next connection ID has overflowed.
		IdOverflow,
		/// The withdrawal timeout has not yet passed.
		TimeoutNotPassed,
	}

	/// Stores the details of each connection, indexed by its unique ID.
	#[pallet::storage]
	#[pallet::getter(fn connections)]
	pub type Connections<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		T::ConnectionId,
		Connection<
			T::AccountId,
			<T::LiquidityRouter as LiquidityRouter<T::AccountId, T::Balance>>::DestinationId,
			T::Balance,
			T::BlockNumber,
		>,
	>;

	/// Stores the next available ID for a new connection.
	#[pallet::storage]
	#[pallet::getter(fn next_connection_id)]
	pub type NextConnectionId<T: Config> = StorageValue<_, T::ConnectionId, ValueQuery>;

	/// Stores stability fee overrides for specific liquidity destinations. If a destination has an
	/// override, it will be used instead of the default stability fee.
	#[pallet::storage]
	#[pallet::getter(fn stability_fee_overrides)]
	pub type StabilityFeeOverrides<T: Config> =
		StorageMap<_, Blake2_128Concat, DestinationIdOf<T>, Rate, OptionQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Opens a new connection to a liquidity destination.
		///
		/// This function allows a user to create a new connection by specifying a liquidity
		/// destination, depositing collateral, and minting a specified amount of stablecoins.
		/// The minted stablecoins are then deposited into the chosen liquidity destination.
		///
		/// # Parameters
		/// - `origin`: The user opening the connection.
		/// - `destination_id`: The ID of the liquidity destination to connect to.
		/// - `collateral_amount`: The amount of collateral to deposit.
		/// - `mint_amount`: The amount of stablecoins to mint.
		#[pallet::call_index(0)]
		#[pallet::weight(Weight::zero())]
		pub fn open_connection(
			origin: OriginFor<T>,
			destination_id: <T::LiquidityRouter as LiquidityRouter<T::AccountId, T::Balance>>::DestinationId,
			collateral_amount: T::Balance,
			mint_amount: T::Balance,
		) -> DispatchResult {
			let owner = ensure_signed(origin)?;

			let connection_id = Self::next_connection_id();
			let next_connection_id =
				connection_id.checked_add(&One::one()).ok_or(Error::<T>::IdOverflow)?;

			T::Currency::transfer(
				&owner,
				&Self::account_id(),
				collateral_amount,
				ExistenceRequirement::KeepAlive,
			)?;

			T::VaultProvider::deposit_collateral(&connection_id, &owner, collateral_amount)
				.map_err(|_| Error::<T>::InsufficientCollateral)?;
			if let Some(stability_fee) = Self::stability_fee_overrides(destination_id) {
				T::VaultProvider::set_stability_fee(&connection_id, Some(stability_fee))?;
			}
			T::VaultProvider::mint(&connection_id, &Self::account_id(), mint_amount)
				.map_err(|_| Error::<T>::MintFailed)?;

			let connection_account = T::PalletId::get().into_sub_account_truncating(connection_id);
			T::LiquidityRouter::deposit(destination_id, &connection_account, mint_amount)
				.map_err(|_| Error::<T>::DepositFailed)?;

			let new_connection = Connection {
				owner: owner.clone(),
				destination_id,
				status: ConnectionStatus::Active,
			};

			Connections::<T>::insert(connection_id, new_connection);
			NextConnectionId::<T>::put(next_connection_id);

			Self::deposit_event(Event::ConnectionOpened {
				connection_id,
				owner,
				destination_id,
				collateral_deposited: collateral_amount,
				stables_minted: mint_amount,
			});

			Ok(())
		}

		/// Initiates a withdrawal of stablecoins from a liquidity destination.
		///
		/// This function begins the process of withdrawing stablecoins from a liquidity
		/// destination. The connection is marked as `WithdrawalInProgress`, and the specified
		/// amount of stablecoins is withdrawn from the destination.
		///
		/// # Parameters
		/// - `origin`: The owner of the connection.
		/// - `connection_id`: The ID of the connection to withdraw from.
		/// - `stables_amount`: The amount of stablecoins to withdraw.
		#[pallet::call_index(1)]
		#[pallet::weight(Weight::zero())]
		pub fn initiate_withdrawal(
			origin: OriginFor<T>,
			connection_id: T::ConnectionId,
			stables_amount: T::Balance,
		) -> DispatchResult {
			let owner = ensure_signed(origin)?;
			let mut connection =
				Self::connections(&connection_id).ok_or(Error::<T>::ConnectionNotFound)?;
			ensure!(owner == connection.owner, Error::<T>::NotOwner);
			ensure!(connection.status == ConnectionStatus::Active, Error::<T>::InvalidStatus);

			connection.status = ConnectionStatus::WithdrawalInProgress {
				amount: stables_amount,
				initiated_at: T::BlockNumberProvider::current_block_number(),
			};

			let connection_account = T::PalletId::get().into_sub_account_truncating(connection_id);
			T::LiquidityRouter::withdraw(
				connection.destination_id,
				&connection_account,
				stables_amount,
			)
			.map_err(|_| Error::<T>::WithdrawalFailed)?;

			Connections::<T>::insert(connection_id, connection);

			Self::deposit_event(Event::WithdrawalInitiated { connection_id, stables_amount });

			Ok(())
		}

		/// Completes a pending withdrawal.
		///
		/// This function is called by the liquidity destination to confirm that the stablecoins
		/// have been released. The stablecoins are then used to repay the debt in the
		/// corresponding vault.
		///
		/// # Parameters
		/// - `origin`: The liquidity destination's account.
		/// - `connection_id`: The ID of the connection.
		/// - `repaid_amount`: The amount of stablecoins that were released.
		#[pallet::call_index(2)]
		#[pallet::weight(Weight::zero())]
		pub fn complete_withdrawal(
			origin: OriginFor<T>,
			connection_id: T::ConnectionId,
			repaid_amount: T::Balance,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let mut connection =
				Self::connections(&connection_id).ok_or(Error::<T>::ConnectionNotFound)?;

			let destination_account =
				T::LiquidityRouter::get_destination_account(connection.destination_id)
					.map_err(|_| Error::<T>::DestinationNotFound)?;
			ensure!(who == destination_account, Error::<T>::BadOrigin);

			if let ConnectionStatus::WithdrawalInProgress { amount, .. } = connection.status {
				ensure!(amount == repaid_amount, Error::<T>::AmountMismatch);
			} else {
				return Err(Error::<T>::InvalidStatus.into());
			}

			T::VaultProvider::repay(&connection_id, &Self::account_id(), repaid_amount)
				.map_err(|_| Error::<T>::RepayFailed)?;

			connection.status = ConnectionStatus::Active;
			Connections::<T>::insert(connection_id, connection);

			Self::deposit_event(Event::WithdrawalCompleted {
				connection_id,
				stables_repaid: repaid_amount,
			});

			Ok(())
		}

		/// Withdraws collateral from a connection's vault.
		///
		/// This allows the owner of a connection to withdraw some of their collateral, as long as
		/// the vault remains sufficiently collateralized.
		///
		/// # Parameters
		/// - `origin`: The owner of the connection.
		/// - `connection_id`: The ID of the connection.
		/// - `amount_to_withdraw`: The amount of collateral to withdraw.
		#[pallet::call_index(3)]
		#[pallet::weight(Weight::zero())]
		pub fn withdraw_collateral(
			origin: OriginFor<T>,
			connection_id: T::ConnectionId,
			amount_to_withdraw: T::Balance,
		) -> DispatchResult {
			let owner = ensure_signed(origin)?;
			let connection =
				Self::connections(&connection_id).ok_or(Error::<T>::ConnectionNotFound)?;
			ensure!(owner == connection.owner, Error::<T>::NotOwner);
			ensure!(connection.status == ConnectionStatus::Active, Error::<T>::InvalidStatus);

			T::Currency::transfer(
				&Self::account_id(),
				&owner,
				amount_to_withdraw,
				ExistenceRequirement::AllowDeath,
			)?;

			T::VaultProvider::withdraw_collateral(&connection_id, &owner, amount_to_withdraw)
				.map_err(|_| Error::<T>::WithdrawCollateralFailed)?;

			Self::deposit_event(Event::CollateralWithdrawn {
				connection_id,
				amount: amount_to_withdraw,
			});

			Ok(())
		}

		/// Closes a connection.
		///
		/// This function withdraws all stablecoins from the liquidity destination, repays the
		/// vault's debt, transfers any remaining stablecoins to the owner, withdraws all
		/// collateral, and closes the vault.
		///
		/// # Parameters
		/// - `origin`: The owner of the connection.
		/// - `connection_id`: The ID of the connection to close.
		#[pallet::call_index(4)]
		#[pallet::weight(Weight::zero())]
		pub fn close_connection(
			origin: OriginFor<T>,
			connection_id: T::ConnectionId,
		) -> DispatchResult {
			let owner = ensure_signed(origin)?;
			let connection =
				Self::connections(&connection_id).ok_or(Error::<T>::ConnectionNotFound)?;
			ensure!(owner == connection.owner, Error::<T>::NotOwner);
			ensure!(connection.status == ConnectionStatus::Active, Error::<T>::InvalidStatus);

			let connection_account = T::PalletId::get().into_sub_account_truncating(connection_id);

			// Withdraw all stablecoins from the liquidity router to the connection account
			T::LiquidityRouter::withdraw_all(connection.destination_id, &connection_account)?;
			let withdrawn_amount =
				T::Fungibles::balance(T::StableCurrencyId::get(), &connection_account);

			// Get vault position
			let (collateral_amount, debt_value) = T::VaultProvider::get_position(&connection_id)
				.map_err(|_| Error::<T>::VaultPosition)?;

			// Repay debt using withdrawn stablecoins
			let amount_to_repay = debt_value.min(withdrawn_amount);
			if !amount_to_repay.is_zero() {
				T::VaultProvider::repay(&connection_id, &connection_account, amount_to_repay)?;
			}

			// Transfer leftover stablecoins to the owner
			let leftover_stables = withdrawn_amount.saturating_sub(amount_to_repay);
			if !leftover_stables.is_zero() {
				T::Fungibles::transfer(
					T::StableCurrencyId::get(),
					&connection_account,
					&owner,
					leftover_stables,
					// We don't care about keeping the connection account alive
					frame_support::traits::tokens::Preservation::Expendable,
				)?;
			}

			// Withdraw all collateral to the owner
			if !collateral_amount.is_zero() {
				T::Currency::transfer(
					&Self::account_id(),
					&owner,
					collateral_amount,
					ExistenceRequirement::AllowDeath,
				)?;
				T::VaultProvider::withdraw_all_collateral(&connection_id, &owner)?;
			}

			T::VaultProvider::close_vault(&connection_id)?;
			Connections::<T>::remove(connection_id);

			Self::deposit_event(Event::ConnectionClosed { connection_id });

			Ok(())
		}

		/// Sets a stability fee override for a specific liquidity destination.
		///
		/// This allows governance to set a custom stability fee for a destination, which will
		/// be applied to all new connections to that destination.
		///
		/// # Parameters
		/// - `origin`: The governance origin.
		/// - `destination_id`: The ID of the destination to set the override for.
		/// - `override_value`: The new stability fee, or `None` to remove the override.
		#[pallet::call_index(5)]
		#[pallet::weight(Weight::zero())] // TODO: Proper benchmarking
		pub fn set_stability_fee(
			origin: OriginFor<T>,
			destination_id: DestinationIdOf<T>,
			override_value: Option<Rate>,
		) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;

			if let Some(value) = override_value {
				StabilityFeeOverrides::<T>::insert(destination_id, value);
			} else {
				StabilityFeeOverrides::<T>::remove(destination_id);
			}

			Self::deposit_event(Event::StabilityFeeOverrideSet {
				destination_id,
				new_value: override_value,
			});

			Ok(())
		}

		#[pallet::call_index(6)]
		#[pallet::weight(Weight::zero())]
		pub fn inject_liquidity(
			origin: OriginFor<T>,
			dest: T::AccountId,
			amount: T::Balance,
		) -> DispatchResult {
			T::GovernanceOrigin::ensure_origin(origin)?;

			let pallet_account = Self::account_id();
			T::Fungibles::mint_into(T::StableCurrencyId::get(), &pallet_account, amount)?;

			T::Fungibles::transfer(
				T::StableCurrencyId::get(),
				&pallet_account,
				&dest,
				amount,
				// We don't care about keeping the pallet account alive
				frame_support::traits::tokens::Preservation::Expendable,
			)?;

			Self::deposit_event(Event::LiquidityInjected {
				destination: dest,
				amount,
			});

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn account_id() -> T::AccountId {
			T::PalletId::get().into_account_truncating()
		}
	}
}
