# Staking Async Parachain

## Overview

This parachain runtime is a fake fork of the asset-hub next (created original by Donál). It is here
to test the async-staking pallet in a real environment.

This parachain contains:

- `pallet-staking-async`
- `pallet-staking-async-rc-client`
- `pallet-election-provider-multi-block` and family
- aux staking pallets `pallet-nomination-pools`, `pallet-fast-unstake`, `pallet-bags-list`, and
  `pallet-delegated-staking`.

All of the above are means to stake and select validators for the RELAY-CHAIN, which is eventually
communicated to it via the `pallet-staking-async-rc-client` pallet.

A lot more is in the runtime, and can be eventually removed.

Note that the parachain runtime also contains a `pallet-session` that works with
`pallet-collator-selection` for the PARACHAIN block author selection.

The counterpart `rc` runtime is a relay chain that is meant to host the parachain. It contains:

- `pallet-staking-async-ah-client`
- `pallet-session`
- `pallet-authorship`
- And all of the consensus pallets that feed the authority set from the session, such as
  aura/babe/grandpa/beefy and so on.

## Run

To run this, a one-click script is provided:

```
bash build-and-run-zn.sh
```

This script will generate chain-specs for both runtimes, and run them with zombie-net.

> Make sure you have all Polkadot binaries (`polkadot`, `polkadot-execution-worker` and
> `polkadot-prepare-worker`) and `polkadot-parachain` installed in your PATH. You can usually
> download them from the Polkadot-sdk release page.

You also need `chain-spec-builder`, but the script builds that and uses a fresh one.

## Configuration

TODO

## Running Benchmarks

TODO
