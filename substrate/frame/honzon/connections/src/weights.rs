#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::{Weight, constants::RocksDbWeight}};
use core::marker::PhantomData;

/// Weight functions needed for pallet_connections.
pub trait WeightInfo {
	fn open_connection() -> Weight;
	fn initiate_withdrawal() -> Weight;
	fn complete_withdrawal() -> Weight;
	fn withdraw_collateral() -> Weight;
	fn close_connection() -> Weight;
	fn set_stability_fee() -> Weight;
	fn inject_liquidity() -> Weight;
}

// For backwards compatibility, we have to provide a value for the weights stored in storage.
impl WeightInfo for () {
	fn open_connection() -> Weight {
		Weight::from_parts(0, 0)
	}
	fn initiate_withdrawal() -> Weight {
		Weight::from_parts(0, 0)
	}
	fn complete_withdrawal() -> Weight {
		Weight::from_parts(0, 0)
	}
	fn withdraw_collateral() -> Weight {
		Weight::from_parts(0, 0)
	}
	fn close_connection() -> Weight {
		Weight::from_parts(0, 0)
	}
	fn set_stability_fee() -> Weight {
		Weight::from_parts(0, 0)
	}
	fn inject_liquidity() -> Weight {
		Weight::from_parts(0, 0)
	}
}
