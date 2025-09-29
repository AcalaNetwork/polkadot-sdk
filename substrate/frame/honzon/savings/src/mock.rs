//! Mocks for the savings pallet.

use super::*;
use crate as pallet_savings;

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	construct_runtime, parameter_types,
	traits::{
		AsEnsureOriginWithArg, ConstU128, ConstU32, ConstU64, EnsureOrigin, Get, SortedMembers,
		tokens::{DepositConsequence, WithdrawConsequence, Fortitude, Provenance, Preservation},
	},
	PalletId,
};
use frame_system::{EnsureSigned, EnsureSignedBy};
use pallet_asset_rewards::FreezeReason;
use scale_info::TypeInfo;
use sp_core::H256;
use frame_support::traits::tokens::fungibles::{self, Mutate, MutateFreeze, Inspect, InspectFreeze};
use frame_support::dispatch::DispatchResult;
use sp_runtime::{
	testing::Header,
	traits::{AccountIdConversion, BlakeTwo256, IdentityLookup},
	BuildStorage, RuntimeDebug,
};


type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlock<Test>;

construct_runtime!(
	pub enum Test where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system,
		Balances: pallet_balances,
		Assets: pallet_assets,
		AssetRewards: pallet_asset_rewards,
		Savings: pallet_savings,
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

impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
		type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = SS58Prefix;
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
	type RuntimeTask = ();
	type ExtensionsWeightInfo = ();
	type SingleBlockMigrations = ();
	type MultiBlockMigrator = ();
	type PreInherents = ();
	type PostInherents = ();
	type PostTransactions = ();
}

parameter_types! {
	pub const ExistentialDeposit: Balance = 1;
}

impl pallet_balances::Config for Test {
	type MaxLocks = ();
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type Balance = Balance;
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = ();
	type FreezeIdentifier = ();
	type MaxFreezes = ();
	type RuntimeHoldReason = ();
	type RuntimeFreezeReason = ();
	type DoneSlashHandler = ();
}

parameter_types! {
	pub const AssetDeposit: Balance = 1;
	pub const ApprovalDeposit: Balance = 1;
	pub const StringLimit: u32 = 50;
	pub const MetadataDepositBase: Balance = 1;
	pub const MetadataDepositPerByte: Balance = 1;
}

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
	type Freezer = ();
	type Extra = ();
	type WeightInfo = ();
	type RemoveItemsLimit = ConstU32<1000>;
	type CallbackHandle = ();
	type Holder = ();
}

parameter_types! {
	pub const AssetRewardsPalletId: PalletId = PalletId(*b"py/asrw ");
}

#[derive(
	Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Encode, Decode, RuntimeDebug, MaxEncodedLen, TypeInfo,
)]
pub enum MyFreezeReason {
	Other,
}

impl From<FreezeReason> for MyFreezeReason {
	fn from(_: FreezeReason) -> Self {
		MyFreezeReason::Other
	}
}

pub struct DummyFreezer;

impl Inspect<AccountId> for DummyFreezer {
    type AssetId = AssetId;
    type Balance = Balance;
    fn total_issuance(asset_id: Self::AssetId) -> Self::Balance {
		Assets::total_issuance(asset_id)
	}
	fn total_balance(asset_id: Self::AssetId, who: &AccountId) -> Self::Balance {
		Assets::total_balance(asset_id, who)
	}
    fn minimum_balance(asset_id: Self::AssetId) -> Self::Balance {
		Assets::minimum_balance(asset_id)
	}
    fn balance(asset_id: Self::AssetId, who: &AccountId) -> Self::Balance {
		Assets::balance(asset_id, who)
	}
    fn reducible_balance(asset_id: Self::AssetId, who: &AccountId, preservation: Preservation, fortitude: Fortitude) -> Self::Balance {
		Assets::reducible_balance(asset_id, who, preservation, fortitude)
	}
    fn can_deposit(asset_id: Self::AssetId, who: &AccountId, amount: Self::Balance, provenance: Provenance) -> DepositConsequence {
		Assets::can_deposit(asset_id, who, amount, provenance)
	}
    fn can_withdraw(asset_id: Self::AssetId, who: &AccountId, amount: Self::Balance) -> WithdrawConsequence<Self::Balance> {
		Assets::can_withdraw(asset_id, who, amount)
	}
	fn asset_exists(asset_id: Self::AssetId) -> bool {
		Assets::asset_exists(asset_id)
	}
}

impl InspectFreeze<AccountId> for DummyFreezer {
    type Id = MyFreezeReason;
    fn balance_frozen(_asset: Self::AssetId, _id: &Self::Id, _who: &AccountId) -> Self::Balance { 0 }
    fn can_freeze(_asset: Self::AssetId, _id: &Self::Id, _who: &AccountId) -> bool { true }
}

impl MutateFreeze<AccountId> for DummyFreezer {
    fn set_freeze(_asset: Self::AssetId, _id: &Self::Id, _who: &AccountId, _amount: Self::Balance) -> DispatchResult { Ok(()) }
    fn extend_freeze(_asset: Self::AssetId, _id: &Self::Id, _who: &AccountId, _amount: Self::Balance) -> DispatchResult { Ok(()) }
    fn thaw(_asset: Self::AssetId, _id: &Self::Id, _who: &AccountId) -> DispatchResult { Ok(()) }
}

impl pallet_asset_rewards::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type AssetId = AssetId;
	type Balance = Balance;
	type Assets = Assets;
	type PalletId = AssetRewardsPalletId;
	type CreatePoolOrigin = EnsureSigned<AccountId>;
	type AssetsFreezer = DummyFreezer;
	type RuntimeFreezeReason = MyFreezeReason;
	type Consideration = ();
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
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

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
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
