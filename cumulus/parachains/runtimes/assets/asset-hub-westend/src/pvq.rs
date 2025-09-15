use codec::{Decode, Encode};
use pvq_extension::{extensions_impl, metadata::Metadata, ExtensionsExecutor, InvokeSource};
use scale_info::TypeInfo;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, TypeInfo)]
pub struct AssetInfo {
	pub asset_id: crate::Vec<u8>,
	pub name: crate::Vec<u8>,
	pub symbol: crate::Vec<u8>,
	pub decimals: u8,
}

#[extensions_impl]
pub mod extensions {
	use alloc::collections::btree_map::BTreeMap;
	use codec::Encode;

	#[extensions_impl::impl_struct]
	pub struct ExtensionImpl;

	#[extensions_impl::extension]
	impl pvq_extension_swap::extension::ExtensionSwap for ExtensionImpl {
		type AssetId = crate::Vec<u8>;
		type Balance = crate::Balance;
		type AssetInfo = super::AssetInfo;
		fn quote_price_tokens_for_exact_tokens(
			asset1: Self::AssetId,
			asset2: Self::AssetId,
			amount: Self::Balance,
			include_fee: bool,
		) -> Option<Self::Balance> {
			if let Ok(asset1) = codec::Decode::decode(&mut &asset1[..]) {
				if let Ok(asset2) = codec::Decode::decode(&mut &asset2[..]) {
					return crate::AssetConversion::quote_price_tokens_for_exact_tokens(
						asset1,
						asset2,
						amount,
						include_fee,
					);
				}
			}
			None
		}

		fn quote_price_exact_tokens_for_tokens(
			asset1: Self::AssetId,
			asset2: Self::AssetId,
			amount: Self::Balance,
			include_fee: bool,
		) -> Option<Self::Balance> {
			if let Ok(asset1) = codec::Decode::decode(&mut &asset1[..]) {
				if let Ok(asset2) = codec::Decode::decode(&mut &asset2[..]) {
					return crate::AssetConversion::quote_price_exact_tokens_for_tokens(
						asset1,
						asset2,
						amount,
						include_fee,
					);
				}
			}
			None
		}

		fn get_liquidity_pool(
			asset1: Self::AssetId,
			asset2: Self::AssetId,
		) -> Option<(Self::Balance, Self::Balance)> {
			if let Ok(asset1) = codec::Decode::decode(&mut &asset1[..]) {
				if let Ok(asset2) = codec::Decode::decode(&mut &asset2[..]) {
					return crate::AssetConversion::get_reserves(asset1, asset2).ok();
				}
			}
			None
		}

		fn list_pools() -> scale_info::prelude::vec::Vec<(Self::AssetId, Self::AssetId)> {
			pallet_asset_conversion::Pools::<crate::Runtime>::iter_keys()
				.map(|pool_id| {
					let (asset1, asset2) = pool_id;
					(asset1.encode(), asset2.encode())
				})
				.collect()
		}

		fn asset_info(asset_id: Self::AssetId) -> Option<Self::AssetInfo> {
			use frame_support::traits::fungibles::Inspect;

			if let Ok(asset_id_decoded) =
				<xcm::v5::Location as codec::Decode>::decode(&mut &asset_id[..])
			{
				// First check if the asset exists in our unified assets system
				if !crate::NativeAndNonPoolAssets::asset_exists(asset_id_decoded.clone()) {
					return None;
				}

				// Check if it's the native token
				if asset_id_decoded == crate::xcm_config::WestendLocation::get() {
					return Some(super::AssetInfo {
						asset_id: asset_id_decoded.encode(),
						name: b"Westend".to_vec(),
						symbol: b"WND".to_vec(),
						decimals: 12,
					});
				}

				// Check trust-backed assets - manual location parsing (known to work)
				if asset_id_decoded.parents == 0 && asset_id_decoded.interior.len() == 2 {
					if let (Some(pallet_junction), Some(index_junction)) =
						(asset_id_decoded.interior.first(), asset_id_decoded.interior.last())
					{
						if let (
							xcm::v5::Junction::PalletInstance(pallet_instance),
							xcm::v5::Junction::GeneralIndex(asset_index),
						) = (pallet_junction, index_junction)
						{
							// Check if this is the trust-backed assets pallet (instance 50)
							if *pallet_instance ==
								<crate::Assets as frame_support::traits::PalletInfoAccess>::index()
									as u8
							{
								let trust_backed_asset_id: crate::AssetIdForTrustBackedAssets =
									(*asset_index).try_into().ok()?;
								if let Ok(metadata) = pallet_assets::Metadata::<
									crate::Runtime,
									crate::TrustBackedAssetsInstance,
								>::try_get(&trust_backed_asset_id)
								{
									return Some(super::AssetInfo {
										asset_id: asset_id_decoded.encode(),
										name: metadata.name.into_inner(),
										symbol: metadata.symbol.into_inner(),
										decimals: metadata.decimals,
									});
								}
							}
						}
					}
				}

				// Try foreign assets
				if let Ok(metadata) = pallet_assets::Metadata::<
					crate::Runtime,
					crate::ForeignAssetsInstance,
				>::try_get(&asset_id_decoded)
				{
					return Some(super::AssetInfo {
						asset_id: asset_id_decoded.encode(),
						name: metadata.name.into_inner(),
						symbol: metadata.symbol.into_inner(),
						decimals: metadata.decimals,
					});
				}

				None
			} else {
				None
			}
		}

		fn assets_info() -> BTreeMap<Self::AssetId, Self::AssetInfo> {
			let mut all_assets = BTreeMap::new();

			// Add native token (WND) - this is handled by the Balances side of
			// NativeAndNonPoolAssets
			let native_location = crate::xcm_config::WestendLocation::get();
			all_assets.insert(
				native_location.encode(),
				super::AssetInfo {
					asset_id: native_location.encode(),
					name: b"Westend".to_vec(),
					symbol: b"WND".to_vec(),
					decimals: 12,
				},
			);

			// Add trust-backed assets - these are part of LocalAndForeignAssets side of
			// NativeAndNonPoolAssets
			for (asset_id, metadata) in
				pallet_assets::Metadata::<crate::Runtime, crate::TrustBackedAssetsInstance>::iter()
			{
				// Convert trust-backed asset ID to location using the same format as the union type
				// expects
				let asset_location = xcm::v5::Location::new(
					0,
					[
						xcm::v5::Junction::PalletInstance(
							<crate::Assets as frame_support::traits::PalletInfoAccess>::index()
								as u8,
						),
						xcm::v5::Junction::GeneralIndex(asset_id.into()),
					],
				);

				all_assets.insert(
					asset_location.encode(),
					super::AssetInfo {
						asset_id: asset_location.encode(),
						name: metadata.name.into_inner(),
						symbol: metadata.symbol.into_inner(),
						decimals: metadata.decimals,
					},
				);
			}

			// Add foreign assets - these are also part of LocalAndForeignAssets side of
			// NativeAndNonPoolAssets
			for (asset_id, metadata) in
				pallet_assets::Metadata::<crate::Runtime, crate::ForeignAssetsInstance>::iter()
			{
				all_assets.insert(
					asset_id.encode(),
					super::AssetInfo {
						asset_id: asset_id.encode(),
						name: metadata.name.into_inner(),
						symbol: metadata.symbol.into_inner(),
						decimals: metadata.decimals,
					},
				);
			}

			all_assets
		}
	}
}

pub fn execute_query(program: &[u8], args: &[u8], gas_limit: i64) -> pvq_primitives::PvqResult {
	let mut executor =
		ExtensionsExecutor::<extensions::Extensions, ()>::new(InvokeSource::RuntimeAPI);
	let (result, _) = executor.execute(program, args, Some(gas_limit));
	result
}

pub fn metadata() -> Metadata {
	extensions::metadata()
}
