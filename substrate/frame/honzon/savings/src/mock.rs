//! Mocks for the savings pallet.

use super::*;
use crate as pallet_savings;

#[cfg(feature = "runtime-benchmarks")]
use crate::BenchmarkHelper;
#[cfg(feature = "runtime-benchmarks")]
use frame_support::assert_ok;
#[cfg(feature = "runtime-benchmarks")]
use frame_support::traits::fungibles::{Inspect as FungiblesInspect, Mutate as FungiblesMutate};
use frame_support::{
	construct_runtime, derive_impl, parameter_types,
	traits::{AsEnsureOriginWithArg, ConstU32, SortedMembers},
	PalletId,
};
use frame_system::{EnsureSigned, EnsureSignedBy};
use sp_core::H256;
use sp_runtime::{
	traits::{AccountIdConversion, BlakeTwo256, IdentityLookup},
	BuildStorage,
};

type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
	pub enum Test{
		System: frame_system,
		Balances: pallet_balances,
		Assets: pallet_assets,
		AssetRewards: pallet_asset_rewards,
		Savings: pallet_savings,
		AssetsFreezer: pallet_assets_freezer,
		AssetsHolder: pallet_assets_holder,
	}
);

pub type AccountId = u128;
pub type Balance = u128;
pub type AssetId = u32;
pub type BlockNumber = u64;

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u8 = 42;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::pallet::DefaultConfig)]
impl frame_system::Config for Test {
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type BlockHashCount = BlockHashCount;
	type AccountData = pallet_balances::AccountData<Balance>;
	type SS58Prefix = SS58Prefix;
	type MaxConsumers = ConstU32<16>;
}

parameter_types! {
	pub const ExistentialDeposit: Balance = 1;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig as pallet_balances::pallet::DefaultConfig)]
impl pallet_balances::Config for Test {
	type ReserveIdentifier = [u8; 8];
	type Balance = Balance;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type MaxFreezes = ConstU32<1>;
}

parameter_types! {
	pub const AssetDeposit: Balance = 1;
	pub const ApprovalDeposit: Balance = 1;
	pub const StringLimit: u32 = 50;
	pub const MetadataDepositBase: Balance = 1;
	pub const MetadataDepositPerByte: Balance = 1;
}

#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig as pallet_assets::pallet::DefaultConfig)]
impl pallet_assets::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Balance = Balance;
	type AssetId = AssetId;
	type AssetIdParameter = u32;
	type Currency = Balances;
	type CreateOrigin = AsEnsureOriginWithArg<EnsureSigned<AccountId>>;
	type ForceOrigin = frame_system::EnsureRoot<AccountId>;
	type AssetDeposit = AssetDeposit;
	type AssetAccountDeposit = AssetDeposit;
	type MetadataDepositBase = MetadataDepositBase;
	type MetadataDepositPerByte = MetadataDepositPerByte;
	type ApprovalDeposit = ApprovalDeposit;
	type StringLimit = StringLimit;
	type Freezer = AssetsFreezer;
	type RemoveItemsLimit = ConstU32<1000>;
	type Holder = AssetsHolder;
}

parameter_types! {
	pub const AssetRewardsPalletId: PalletId = PalletId(*b"py/asrw ");
}

impl pallet_assets_freezer::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeFreezeReason = RuntimeFreezeReason;
}

impl pallet_assets_holder::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeHoldReason = RuntimeHoldReason;
}

#[cfg(feature = "runtime-benchmarks")]
pub struct AssetRewardsBenchHelper;

#[cfg(feature = "runtime-benchmarks")]
impl pallet_asset_rewards::benchmarking::BenchmarkHelper<AssetId> for AssetRewardsBenchHelper {
	fn staked_asset() -> AssetId {
		1
	}

	fn reward_asset() -> AssetId {
		2
	}
}

impl pallet_asset_rewards::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type AssetId = AssetId;
	type Balance = Balance;
	type Assets = Assets;
	type PalletId = AssetRewardsPalletId;
	type CreatePoolOrigin = EnsureSigned<AccountId>;
	type AssetsFreezer = AssetsFreezer;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type Consideration = ();
	type WeightInfo = ();
	type BlockNumberProvider = System;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = AssetRewardsBenchHelper;
}

parameter_types! {
	pub const SavingsPalletId: PalletId = PalletId(*b"py/svngs");
	pub const UpdatePeriod: BlockNumber = 100;
	pub const MaxRewardPools: u32 = 10;
}

pub struct AdminAccount;
impl SortedMembers<AccountId> for AdminAccount {
	fn sorted_members() -> Vec<AccountId> {
		vec![1]
	}
}

#[cfg(feature = "runtime-benchmarks")]
pub struct BenchHelper;

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkHelper<AccountId, AssetId, Balance> for BenchHelper {
	fn prepare_pool(
		_pool_index: u32,
		savings_account: &AccountId,
		reward_budget: Balance,
	) -> (AssetId, AssetId, Option<AccountId>) {
		let staked_asset_id = 1;
		let reward_asset_id = 2;
		let required = reward_budget
			.saturating_add(<Assets as FungiblesInspect<_>>::minimum_balance(reward_asset_id));
		assert_ok!(<Assets as FungiblesMutate<_>>::mint_into(
			reward_asset_id,
			savings_account,
			required
		));
		(staked_asset_id, reward_asset_id, Some(1))
	}
}

impl Config for Test {
	type Balance = Balance;
	type AssetId = AssetId;
	type Assets = Assets;
	type UpdateOrigin = EnsureSignedBy<AdminAccount, AccountId>;
	type UpdatePeriod = UpdatePeriod;
	type BlockNumberProvider = System;
	type RewardPool = AssetRewards;
	type PalletId = SavingsPalletId;
	type PoolId = u32;
	type AssetRewardsPalletId = AssetRewardsPalletId;
	type MaxRewardPools = MaxRewardPools;
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = BenchHelper;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(1, 100), (2, 100)],
		dev_accounts: Default::default(),
	}
	.assimilate_storage(&mut t)
	.unwrap();

	pallet_assets::GenesisConfig::<Test> {
		assets: vec![
			(1, 1, true, 1), // Staked Asset
			(2, 1, true, 1), // Reward Asset
		],
		metadata: vec![],
		next_asset_id: Some(3),
		accounts: vec![(2, SavingsPalletId::get().into_account_truncating(), 1_000_000)],
	}
	.assimilate_storage(&mut t)
	.unwrap();

	t.into()
}
