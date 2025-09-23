# CDP Engine Pallet

## Overview

The CDP Engine pallet is the core component of the Honzon protocol, responsible for managing Collateralized Debt Positions (CDPs). It handles the internal processes of CDPs, including liquidation, settlement, and risk management.

This pallet works in conjunction with `pallet-loans` to manage the collateral and debt of each position, `pallet-dex` for liquidating collateral, and an oracle for price feeds.

## Key Concepts

*   **Collateralized Debt Position (CDP):** A CDP is a loan where a user locks up collateral (e.g., DOT) to borrow a stablecoin (e.g., aUSD).
*   **Liquidation:** If the value of the collateral drops and the collateral-to-debt ratio falls below a certain threshold (the liquidation ratio), the CDP is considered unsafe and can be liquidated. During liquidation, the collateral is sold to cover the debt, plus a penalty.
*   **Settlement:** In the case of a global shutdown of the system, this pallet handles the settlement of all outstanding CDPs.
*   **Risk Management:** The pallet includes several parameters to manage the risk of the system, such as liquidation ratios, liquidation penalties, and debt ceilings for each collateral type.

## Extrinsics

*   `liquidate`: Liquidates an unsafe CDP. This is an unsigned extrinsic that can be called by anyone, and is typically triggered by an offchain worker.
*   `settle`: Settles a CDP after a global shutdown. This is also an unsigned extrinsic.
*   `set_collateral_params`: Updates the risk management parameters for a collateral type. This is a privileged extrinsic that can only be called by a specified origin.

## Offchain Worker

The pallet includes an offchain worker that monitors the state of all CDPs. If a CDP becomes unsafe, the offchain worker submits an unsigned `liquidate` extrinsic to liquidate the position.
