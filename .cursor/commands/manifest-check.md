Check the manifest of the pallet:
1. All dependencies should reference the workspace's dependencies using `workspace = true`.
2. Since workspace's dependencies already have `default-features = false`, `default-features = false` is not necessary to add to.
3. Dependencies that only used in tests (`#![cfg(test)]`) should be added to `[dev-dependencies]` instead of `[dependencies]`.
4. Use `zepter` cli to check the features are propagated correctly in the manifest. The command is `zepter lint propagate-feature --features <FEATURES> -p PALLET_NAME`. (i.e. `zepter lint propagate-feature --features try-runtime,runtime-benchmarks,std -p pallet-connections`)
5. Use `cargo test -p PALLET_NAME` to check the pallet compiles and runs the tests.
