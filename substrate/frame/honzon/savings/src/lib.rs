//! A pallet for managing savings pools.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{
		pallet_prelude::*,
		rewards::RewardsPool,
		traits::{fungibles::{self, Mutate}, tokens::Preservation, EnsureOrigin, Get},
		PalletId,
	};
	use frame_system::pallet_prelude::*;
	use scale_info::TypeInfo;
	use sp_runtime::traits::{AccountIdConversion, AtLeast32BitUnsigned, BlockNumberProvider, Saturating};

	#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, RuntimeDebug)]
	pub struct PoolData<PoolId, AssetId, Balance> {
		pub pool_id: PoolId,
		pub reward_asset_id: AssetId,
		pub reward_rate_per_block: Balance,
	}

	type BlockNumberFor<T> =
	<<T as Config>::BlockNumberProvider as BlockNumberProvider>::BlockNumber;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The type of balance.
		type Balance: Parameter
			+ Member
			+ AtLeast32BitUnsigned
			+ Default
			+ Copy
			+ MaxEncodedLen
			+ From<BlockNumberFor<Self>>;

		/// The type of asset id.
		type AssetId: Parameter + Member + Default + Copy + MaxEncodedLen;

		/// Something that provides fungible access.
		type Assets: fungibles::Mutate<Self::AccountId, AssetId = Self::AssetId, Balance = Self::Balance>;

		/// The origin which can create new pools.
		type UpdateOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The period for which new pools are created.
		type UpdatePeriod: Get<BlockNumberFor<Self>>;

		/// Something that provides the current block number.
		type BlockNumberProvider: BlockNumberProvider;

		/// The reward pool trait.
		type RewardPool: RewardsPool<
			Self::AccountId,
			Self::PoolId,
			Self::Balance,
			AssetId = Self::AssetId,
			BlockNumber = BlockNumberFor<Self>,
		>;

		/// The pallet id for this pallet.
		#[pallet::constant]
		type PalletId: Get<PalletId>;

		/// The type of a pool id.
		type PoolId: Parameter + MaxEncodedLen + Copy + Default;

		/// The pallet id for the asset rewards pallet.
		#[pallet::constant]
		type AssetRewardsPalletId: Get<PalletId>;

		/// The maximum number of reward pools that can be created.
		#[pallet::constant]
		type MaxRewardPools: Get<u32>;
	}

	#[pallet::storage]
	#[pallet::getter(fn reward_pools)]
	pub type RewardPools<T: Config> =
		StorageValue<_, BoundedVec<PoolData<T::PoolId, T::AssetId, T::Balance>, T::MaxRewardPools>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new pool was created.
		PoolCreated { pool_id: T::PoolId },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The maximum number of pools has been reached.
		TooManyPools,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a new reward pool.
		#[pallet::call_index(0)]
		#[pallet::weight(T::DbWeight::get().writes(1))]
		pub fn create_pool(
			origin: OriginFor<T>,
			staked_asset_id: T::AssetId,
			reward_asset_id: T::AssetId,
			reward_rate_per_block: T::Balance,
			admin: Option<T::AccountId>,
		) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;

			let now = T::BlockNumberProvider::current_block_number();
			let expiry = now + T::UpdatePeriod::get();

			let sovereign_account = T::PalletId::get().into_account_truncating();

			let pool_id = T::RewardPool::create_pool(
				&sovereign_account,
				staked_asset_id,
				reward_asset_id,
				reward_rate_per_block,
				frame_support::traits::schedule::DispatchTime::At(expiry),
				admin,
			)?;

			let pool_data = PoolData {
				pool_id,
				reward_asset_id,
				reward_rate_per_block,
			};

			RewardPools::<T>::try_append(pool_data).map_err(|_| Error::<T>::TooManyPools)?;

			Self::deposit_event(Event::PoolCreated { pool_id });

			Ok(())
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<frame_system::pallet_prelude::BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(_now: frame_system::pallet_prelude::BlockNumberFor<T>) -> Weight {
			let now = T::BlockNumberProvider::current_block_number();
			if (now % T::UpdatePeriod::get()).is_zero() {
				let pools = RewardPools::<T>::get();
				for pool in pools {
					let period = T::Balance::from(T::UpdatePeriod::get());
					let amount = pool.reward_rate_per_block.saturating_mul(period);
					let savings_account = T::PalletId::get().into_account_truncating();
					let pool_account = T::AssetRewardsPalletId::get().into_sub_account_truncating(pool.pool_id);
					let _ = T::Assets::transfer(
						pool.reward_asset_id,
						&savings_account,
						&pool_account,
						amount,
						Preservation::Preserve,
					);
				}
			}
			Weight::zero()
		}
	}
}

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
