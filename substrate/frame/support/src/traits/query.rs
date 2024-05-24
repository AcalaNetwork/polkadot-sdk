// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License fsor the specific language governing permissions and
// limitations under the License.

//! Traits for querying pallet view functions.

use codec::{Decode, Encode};

/// implemented by the runtime dispatching by prefix and then the pallet dispatching by suffix
pub trait DispatchQuery {
	fn dispatch_query(id: &QueryId, input: Vec<u8>) -> Result<Vec<u8>, codec::Error>;
}

pub trait QueryIdPrefix {
	const PREFIX: [u8; 16]; // same as `PalletInfo::name_hash` twox_128
}

pub trait QueryIdSuffix {
	const SUFFIX: [u8; 16];
}

#[derive(Encode, Decode)]
pub struct QueryId {
	prefix: [u8; 16],
	suffix: [u8; 16],
}

/// implemented for each pallet view function method
pub trait Query {
	const ID: QueryId;
	type ReturnType: codec::Codec;

	fn query(self) -> Self::ReturnType;
}
