# pUSD — Requirements Specification

## 1. Overview

- pUSD is a Polkadot‑native, over‑collateralized stablecoin deployed on Asset Hub.

## 2. Collateral Rules

- Collateral can be used for governance voting.
- Collateral cannot be used for staking.

## 3. Governance & Liquidation

- If collateral is liquidated:
  - Any governance votes backed by that collateral must be removed via the Task system.
  - When a vault is confiscated, conviction voting holds/locks are ignored.

## 4. Collateralization Ratios

- Required Collateral Ratio (RCR): a vault can mint pUSD up to the RCR.
- Liquidation Ratio (LR): a vault is liquidated if its collateral ratio falls below the LR.

## 5. Connections & Liquidity Management

### 5.1 Provisioning & Authorization

- Users may create a connection by depositing DOT and setting a destination (e.g., a parachain endpoint or smart contract).

### 5.2 Connection‑Specific Collateral Policy

- Governance may define a special Connection RCR for each connection type.
- When a connection opens a vault, the effective RCR is `min(Connection_RCR, Default_RCR)`.
- Special rates (e.g., modified collateral ratios) always require governance approval.

### 5.3 Minting, Routing, and Band Rebalancing

- Each connection opens its own vault.
- The connection mints pUSD up to the effective RCR and sends it to the destination.

#### Collateral band per connection

- Each connection has a collateral band defined by two ratios (defaults in parentheses):
  - Upper Collateral Ratio (UCR_conn) — default: RCR.
  - Lower Collateral Ratio (LCR_conn) — default: LR.
- Mid‑point target: `midpoint := (UCR_conn + LCR_conn) / 2`.
- On connection creation, the system mints pUSD so the vault starts at the mid‑point collateral ratio.
- Rebalancing rule:
  - If the connection vault’s collateral ratio moves outside the band `[LCR_conn, UCR_conn]`, the system performs:
    - Inject: mint pUSD and route it to the destination.
    - Withdraw: withdraw pUSD from the destination and repay the vault debt.
  - Goal: steer the vault’s collateral ratio back to the mid‑point.
- Note (non‑liquidation assumption): if the destination is risk‑free and its guaranteed interest rate exceeds the vault’s stability fee, the connection is assumed non‑liquidatable (net earnings outpace debt growth; rebalancing keeps the ratio in range).

#### Multi‑connection support

- Users may have multiple independent connections, each with its own vault, destination, and parameters.

### 5.4 User Operations

- Open: create a connection by depositing DOT and defining a destination.
- Top‑up: add more DOT collateral to the connection vault.
- Withdraw: request to unlock DOT; the unlock is processed only after a successful pUSD withdraw/repay, which is asynchronous and may take several blocks.
- Close: close a connection after all obligations are settled.
- Restriction: users cannot directly access or move assets owned by the connection — they interact only through the defined operations.

### 5.5 Automatic Actions

- Automatic mint (optional): the vault automatically mints pUSD up to the RCR.
- Automatic repay (optional): when near the LR, the protocol withdraws liquidity via connections and repays debt.
- Triggering order: repayment is attempted before liquidation.

### 5.6 Asynchronous Operations & Grace

- Withdrawals are asynchronous, with funds arriving several blocks later (supports XCM or CEX integration).
- During an asynchronous withdrawal, the vault has a grace window (e.g., 10 blocks) in which it cannot be liquidated.

### 5.7 Connection Definition & Evolution

- Current model: connections are defined in runtime code (governance‑curated set).
- Future model: with smart contract integration, users will be able to create connections via smart contracts, enabling permissionless connection registration.
- Even in the permissionless model, special rates (e.g., custom collateral ratios) still require governance approval.

## 6. Automation & Monitoring

- An offchain worker monitors vault collateral ratios and triggers:
  - Automatic connection mint/repay.
  - Liquidation if below LR.
- Future optimization: reduce reliance on offchain workers (e.g., a priority queue of vaults).

## 7. PIB — Polkadot Issuance Buffer

- PIB can open pUSD vaults with zero stability fee.
- During liquidation, PIB may issue pUSD to participate.
- Open question: should there be a special origin that can always open a vault with zero stability fee?

## 8. Vault Stability Fee

- Each vault has a stability fee set to the rate active when it last minted pUSD.
- Governance override (per‑vault): governance may override the stability fee for a specific vault.
  - If an override is set, it replaces the vault’s set fee for the purposes of the effective‑fee calculation.
- Effective stability fee: `min(vault.stability_fee, current_stability_fee)`.
- Guarantees:
  - Predictability: a vault won’t pay more than its set or overridden fee.
  - Fairness: vaults benefit from global fee reductions.

## 9. Probable‑Oracle (Price Feed)

- Offchain worker process:
  - Select X sources (CEX/API) → compute volume‑weighted median (VWM).
  - Compare to the last on‑chain price and compute the difference.
  - Map the difference to an UP probability.
  - Flip a weighted coin (UP/DOWN) and submit a signed vote.
- On‑chain aggregation:
  - With `Ups`, `Downs`, and rate `R`:
    - `new_price := old_price + (Ups - Downs) * R`
- Bounded change:
  - With `O` oracles, the max per‑block swing is `R * O`.
  - This bound allows participants to predict worst‑case price changes during asynchronous rebalancing and avoid systemic instability.
