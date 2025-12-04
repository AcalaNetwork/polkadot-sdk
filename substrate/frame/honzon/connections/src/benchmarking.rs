#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::Pallet as Connections;
use frame_benchmarking::{v2::*, whitelisted_caller};
use frame_support::{
	assert_ok,
	traits::{honzon::Rate, Get},
};
use frame_system::RawOrigin;
use sp_runtime::{traits::AccountIdConversion, FixedPointNumber};

/// Helper trait for preparing benchmarking state.
pub trait BenchmarkHelper<AccountId, Balance, ConnectionId, DestinationId, BlockNumber> {
	/// Returns a destination identifier that can be used during benchmarking.
	fn destination_id(seed: u32) -> DestinationId;

	/// Returns the collateral amount that should be used when opening a connection.
	fn collateral_amount() -> Balance;

	/// Returns the stablecoin amount that should be minted when opening a connection.
	fn mint_amount() -> Balance;

	/// Prepare the environment so that `open_connection` can succeed.
	///
	/// Implementations may create the destination, ensure the caller has sufficient collateral,
	/// and perform any other setup required by the runtime.
	fn prepare_open(
		owner: &AccountId,
		connection_id: ConnectionId,
		connection_account: &AccountId,
		destination_id: DestinationId,
		collateral_amount: Balance,
		mint_amount: Balance,
	) -> sp_runtime::DispatchResult;

	/// Set the current block number to the provided value.
	fn set_block_number(block: BlockNumber);
}

fn connection_account<T: Config>(connection_id: T::ConnectionId) -> T::AccountId {
	T::PalletId::get().into_sub_account_truncating(connection_id)
}

fn setup_connection<T: Config>(
	owner: &T::AccountId,
	destination_seed: u32,
) -> Result<
	(T::ConnectionId, DestinationIdOf<T>, T::Balance, T::Balance, T::AccountId),
	BenchmarkError,
> {
	let connection_id = NextConnectionId::<T>::get();
	let connection_account = connection_account::<T>(connection_id);
	let destination_id = T::BenchmarkHelper::destination_id(destination_seed);
	let collateral = T::BenchmarkHelper::collateral_amount();
	let mint = T::BenchmarkHelper::mint_amount();

	T::BenchmarkHelper::prepare_open(
		owner,
		connection_id,
		&connection_account,
		destination_id,
		collateral,
		mint,
	)
	.map_err(|_| BenchmarkError::Weightless)?;

	assert_ok!(Connections::<T>::open_connection(
		RawOrigin::Signed(owner.clone()).into(),
		destination_id,
		collateral,
		mint,
	));

	Ok((connection_id, destination_id, collateral, mint, connection_account))
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn open_connection() -> Result<(), BenchmarkError> {
		let owner: T::AccountId = whitelisted_caller();
		let connection_id = NextConnectionId::<T>::get();
		let connection_account = connection_account::<T>(connection_id);
		let destination_id = T::BenchmarkHelper::destination_id(0);
		let collateral = T::BenchmarkHelper::collateral_amount();
		let mint = T::BenchmarkHelper::mint_amount();

		T::BenchmarkHelper::prepare_open(
			&owner,
			connection_id,
			&connection_account,
			destination_id,
			collateral,
			mint,
		)
		.map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(owner.clone()), destination_id, collateral, mint);

		Ok(())
	}

	#[benchmark]
	fn initiate_withdrawal() -> Result<(), BenchmarkError> {
		let owner: T::AccountId = whitelisted_caller();
		let (connection_id, _destination_id, _, mint, _) = setup_connection::<T>(&owner, 1)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(owner.clone()), connection_id, mint);

		Ok(())
	}

	#[benchmark]
	fn complete_withdrawal() -> Result<(), BenchmarkError> {
		let owner: T::AccountId = whitelisted_caller();
		let (connection_id, destination_id, _, mint, _) = setup_connection::<T>(&owner, 2)?;

		assert_ok!(Connections::<T>::initiate_withdrawal(
			RawOrigin::Signed(owner.clone()).into(),
			connection_id,
			mint,
		));

		let destination_account = T::LiquidityRouter::get_destination_account(destination_id)
			.map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(destination_account), connection_id, mint);

		Ok(())
	}

	#[benchmark]
	fn withdraw_collateral() -> Result<(), BenchmarkError> {
		let owner: T::AccountId = whitelisted_caller();
		let (connection_id, _destination_id, collateral, _, _) = setup_connection::<T>(&owner, 3)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(owner.clone()), connection_id, collateral);

		Ok(())
	}

	#[benchmark]
	fn close_connection() -> Result<(), BenchmarkError> {
		let owner: T::AccountId = whitelisted_caller();
		let (connection_id, _destination_id, _collateral, _mint, _) =
			setup_connection::<T>(&owner, 4)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(owner.clone()), connection_id);

		Ok(())
	}

	#[benchmark]
	fn set_stability_fee() -> Result<(), BenchmarkError> {
		let owner: T::AccountId = whitelisted_caller();
		let (_connection_id, destination_id, _collateral, _mint, _account) =
			setup_connection::<T>(&owner, 5)?;
		let rate = Rate::saturating_from_integer(1);

		#[extrinsic_call]
		_(RawOrigin::Root, destination_id, Some(rate));

		Ok(())
	}

	#[benchmark]
	fn inject_liquidity() -> Result<(), BenchmarkError> {
		let dest: T::AccountId = whitelisted_caller();
		let amount = T::BenchmarkHelper::mint_amount();

		#[extrinsic_call]
		_(RawOrigin::Root, dest, amount);

		Ok(())
	}

	impl_benchmark_test_suite!(Connections, crate::mock::new_test_ext(), crate::mock::Test);
}
