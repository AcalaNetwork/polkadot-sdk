# Auction Manager Pallet

## Overview

The Auction Manager pallet is a key component of the Honzon protocol, responsible for managing the auctioning of system assets to ensure its operational stability. It orchestrates different types of auctions, with a primary focus on collateral auctions. These auctions are initiated to sell collateral assets in exchange for a stable currency, effectively mitigating the system's bad debt.

The pallet defines the rules and processes for these auctions, including their creation, cancellation, and settlement. It works in close conjunction with the `pallet-auction` to handle the underlying auction mechanics, such as bidding and winner determination.

## Interface

### Dispatchable Functions

The Auction Manager pallet does not expose any dispatchable functions. Its operations are primarily triggered by other pallets within the Honzon protocol.

### Public Functions

The pallet provides several public functions through the `AuctionManager` trait, allowing other pallets to interact with the auction process:

-   `new_collateral_auction(refund_recipient, currency_id, amount, target)`: Creates a new collateral auction.
-   `cancel_auction(id)`: Cancels an existing auction.
-   `get_total_collateral_in_auction(id)`: Retrieves the total amount of a specific collateral type currently in auction.
-   `get_total_target_in_auction()`: Retrieves the total target amount for all ongoing auctions.

## Usage

The Auction Manager pallet is designed to be used internally by the Honzon protocol. For example, the CDP Engine might trigger a collateral auction when a position becomes unsafe.

## Dependencies

This pallet depends on the following pallets:

-   `pallet-auction`: Provides the core auction functionality.
-   `pallet-cdp-treasury`: Manages the treasury and surplus/deficit funds.
-   `pallet-oracle`: Provides price feeds for assets.
-   `pallet-traits`: Defines shared traits used across the Honzon protocol.
