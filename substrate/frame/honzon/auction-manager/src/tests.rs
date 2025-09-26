// This file is part of Acala.

// Copyright (C) 2020-2025 Acala Foundation.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Unit tests for the auction manager module.

#![cfg(test)]

use super::pallet::Error;
use crate::{
	mock::{
		mock_shutdown, AccountId, Assets, AuctionId, AuctionManagerModule, AuctionModule, Balance,
		BlockNumber, CDPTreasuryModule, CurrencyId, ExtBuilder, MockPriceSource, Runtime,
		RuntimeEvent, RuntimeOrigin, System, TreasuryAccount, ALICE, BOB, CAROL, NATIVE, STABLE,
	},
	pallet::{CollateralAuctions, Event, TotalCollateralInAuction, TotalTargetInAuction},
};
use frame_support::{
	assert_noop, assert_ok,
	traits::{OnInitialize, OriginTrait},
};
use pallet_traits::{AuctionHandler, AuctionManager, CDPTreasury, Rate};
use sp_runtime::{
	traits::{BadOrigin, One},
	FixedPointNumber,
};

// #[test]
// fn get_auction_time_to_close_work() {
// 	ExtBuilder::default().build().execute_with(|| {
// 		assert_eq!(AuctionManagerModule::get_auction_time_to_close(2000, 1), 100);
// 		assert_eq!(AuctionManagerModule::get_auction_time_to_close(2001, 1), 50);
// 	});
// }

#[test]
fn collateral_auction_methods() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(<AuctionManagerModule as AuctionManager<AccountId>>::new_collateral_auction(
			&ALICE, NATIVE, 10, 100,
		));
		let auction = AuctionModule::auctions(0).unwrap();
		assert_eq!(auction.bid, None);
		assert_eq!(auction.start, 0);
		assert_eq!(auction.end, None);
		let collateral_auction_with_positive_target =
			CollateralAuctions::<Runtime>::get(0).unwrap();
		assert!(!collateral_auction_with_positive_target.always_forward());
		assert!(
			!collateral_auction_with_positive_target.in_reverse_stage(Rate::from_rational(9, 1))
		);
		assert!(
			collateral_auction_with_positive_target.in_reverse_stage(Rate::from_rational(10, 1))
		);
		assert!(
			collateral_auction_with_positive_target.in_reverse_stage(Rate::from_rational(11, 1))
		);
		assert_eq!(
			collateral_auction_with_positive_target.payment_amount(Rate::from_rational(9, 1)),
			90
		);
		assert_eq!(
			collateral_auction_with_positive_target.payment_amount(Rate::from_rational(10, 1)),
			100
		);
		assert_eq!(
			collateral_auction_with_positive_target.payment_amount(Rate::from_rational(11, 1)),
			100
		);

		assert_ok!(<AuctionManagerModule as AuctionManager<AccountId>>::new_collateral_auction(
			&ALICE, NATIVE, 10, 0,
		));
		let collateral_auction_with_zero_target = CollateralAuctions::<Runtime>::get(1).unwrap();
		assert!(collateral_auction_with_zero_target.always_forward());
		assert!(!collateral_auction_with_zero_target.in_reverse_stage(Rate::from_rational(0, 1)));
		assert!(!collateral_auction_with_zero_target.in_reverse_stage(Rate::from_rational(100, 1)));
		assert_eq!(
			collateral_auction_with_zero_target.payment_amount(Rate::from_rational(99, 1)),
			990
		);
		assert_eq!(
			collateral_auction_with_zero_target.payment_amount(Rate::from_rational(101, 1)),
			1010
		);
	});
}

#[test]
fn new_collateral_auction_work() {
	ExtBuilder::default().build().execute_with(|| {
		frame_system::Pallet::<Runtime>::set_block_number(1);
		let ref_count_0 = frame_system::Pallet::<Runtime>::consumers(&ALICE);
		assert_noop!(
			<AuctionManagerModule as AuctionManager<AccountId>>::new_collateral_auction(
				&ALICE, NATIVE, 0, 100
			),
			Error::<Runtime>::InvalidAmount,
		);

		assert_ok!(<AuctionManagerModule as AuctionManager<AccountId>>::new_collateral_auction(
			&ALICE, NATIVE, 10, 100,
		));
		System::assert_last_event(RuntimeEvent::AuctionManagerModule(
			Event::NewCollateralAuction {
				auction_id: 0,
				collateral_type: NATIVE,
				collateral_amount: 10,
				target_bid_price: 100,
			},
		));

		assert_eq!(TotalCollateralInAuction::<Runtime>::get(), 10);
		assert_eq!(TotalTargetInAuction::<Runtime>::get(), 100);
		assert_eq!(AuctionModule::auctions_index(), 1);
		assert_eq!(frame_system::Pallet::<Runtime>::consumers(&ALICE), ref_count_0);

		assert_noop!(
			<AuctionManagerModule as AuctionManager<AccountId>>::new_collateral_auction(
				&ALICE,
				NATIVE,
				Balance::max_value(),
				Balance::max_value(),
			),
			Error::<Runtime>::InvalidAmount
		);
	});
}

#[test]
fn collateral_auction_bid_handler_work() {
	ExtBuilder::default().build().execute_with(|| {
		frame_system::Pallet::<Runtime>::set_block_number(1);
		assert_noop!(
			AuctionModule::bid(RuntimeOrigin::signed(BOB), 0, 4),
			pallet_auction::Error::<Runtime>::AuctionNotExist,
		);

		assert_ok!(<AuctionManagerModule as AuctionManager<AccountId>>::new_collateral_auction(
			&ALICE, NATIVE, 10, 100,
		));
		assert_eq!(CDPTreasuryModule::surplus_pool(), 0);
		assert_eq!(Assets::balance(STABLE, &BOB), 1000);

		assert_noop!(
			AuctionModule::bid(RuntimeOrigin::signed(BOB), 0, 40),
			pallet_auction::Error::<Runtime>::BidNotAccepted,
		);
		assert_ok!(AuctionModule::bid(RuntimeOrigin::signed(BOB), 0, 50));
		assert_eq!(CDPTreasuryModule::surplus_pool(), 50);
		assert_eq!(Assets::balance(STABLE, &BOB), 950);

		assert_ok!(AuctionModule::bid(RuntimeOrigin::signed(CAROL), 0, 100));
		assert_eq!(CDPTreasuryModule::surplus_pool(), 100);
		assert_eq!(Assets::balance(STABLE, &BOB), 1000);
		assert_eq!(Assets::balance(STABLE, &CAROL), 900);
		assert_eq!(CollateralAuctions::<Runtime>::get(0).unwrap().amount, 10);

		assert_ok!(AuctionModule::bid(RuntimeOrigin::signed(BOB), 0, 200));
		assert_eq!(CDPTreasuryModule::surplus_pool(), 100);
		assert_eq!(Assets::balance(STABLE, &BOB), 900);
		assert_eq!(Assets::balance(STABLE, &CAROL), 1000);
		assert_eq!(CollateralAuctions::<Runtime>::get(0).unwrap().amount, 5);
	});
}

#[test]
fn bid_when_soft_cap_for_collateral_auction_work() {
	ExtBuilder::default().build().execute_with(|| {
		frame_system::Pallet::<Runtime>::set_block_number(1);
		assert_ok!(<AuctionManagerModule as AuctionManager<AccountId>>::new_collateral_auction(
			&ALICE, NATIVE, 10, 100,
		));
		assert_ok!(AuctionModule::bid(RuntimeOrigin::signed(BOB), 0, 100));
		assert_eq!(AuctionModule::auctions(0).unwrap().end, Some(101));

		frame_system::Pallet::<Runtime>::set_block_number(2001);

		assert_noop!(
			AuctionModule::bid(RuntimeOrigin::signed(CAROL), 0, 100),
			pallet_auction::Error::<Runtime>::InvalidBidPrice
		);
		assert_ok!(AuctionModule::bid(RuntimeOrigin::signed(CAROL), 0, 110));
		assert_eq!(AuctionModule::auctions(0).unwrap().end, Some(2051));
	});
}

#[test]
fn always_forward_collateral_auction_without_bid_aborted() {
	ExtBuilder::default().with_treasury_collateral(100).build().execute_with(|| {
		frame_system::Pallet::<Runtime>::set_block_number(1);
		assert_ok!(<AuctionManagerModule as AuctionManager<AccountId>>::new_collateral_auction(
			&CDPTreasuryModule::account_id(),
			NATIVE,
			100,
			0,
		));
		assert_eq!(TotalCollateralInAuction::<Runtime>::get(), 100);
		assert_eq!(CDPTreasuryModule::surplus_pool(), 0);
		let ref_count_0 = frame_system::Pallet::<Runtime>::consumers(&CDPTreasuryModule::account_id());

		<AuctionManagerModule as AuctionHandler<AccountId, Balance, BlockNumber, AuctionId>>::on_auction_ended(0, None);
		System::assert_last_event(RuntimeEvent::AuctionManagerModule(
			Event::CollateralAuctionAborted {
				auction_id: 0,
				collateral_type: NATIVE,
				collateral_amount: 100,
				target_stable_amount: 0,
				refund_recipient: CDPTreasuryModule::account_id(),
			},
		));

		assert_eq!(<CDPTreasuryModule as CDPTreasury<AccountId>>::get_total_collaterals(), 100);
		assert_eq!(TotalCollateralInAuction::<Runtime>::get(), 0);
		assert_eq!(CDPTreasuryModule::surplus_pool(), 0);
		let ref_count_1 = frame_system::Pallet::<Runtime>::consumers(&CDPTreasuryModule::account_id());
		assert_eq!(ref_count_1, ref_count_0);
	});
}

#[test]
fn always_forward_collateral_auction_dealt() {
	ExtBuilder::default().with_treasury_collateral(100).build().execute_with(|| {
		frame_system::Pallet::<Runtime>::set_block_number(1);
		assert_ok!(<AuctionManagerModule as AuctionManager<AccountId>>::new_collateral_auction(
			&CDPTreasuryModule::account_id(),
			NATIVE,
			100,
			0,
		));
		assert_ok!(AuctionModule::bid(RuntimeOrigin::signed(BOB), 0, 2));
		assert_eq!(TotalCollateralInAuction::<Runtime>::get(), 100);
		assert_eq!(CDPTreasuryModule::surplus_pool(), 200);
		assert_eq!(Assets::balance(STABLE, &BOB), 800);
		let ref_count_0 = frame_system::Pallet::<Runtime>::consumers(&CDPTreasuryModule::account_id());
		let bob_ref_count_0 = frame_system::Pallet::<Runtime>::consumers(&BOB);

		<AuctionManagerModule as AuctionHandler<AccountId, Balance, BlockNumber, AuctionId>>::on_auction_ended(0, Some((BOB, 2)));
		System::assert_last_event(RuntimeEvent::AuctionManagerModule(
			Event::CollateralAuctionDealt {
				auction_id: 0,
				collateral_type: NATIVE,
				collateral_amount: 100,
				winner: BOB,
				payment_amount: 200,
			},
		));
		assert_eq!(TotalCollateralInAuction::<Runtime>::get(), 0);
		assert_eq!(CDPTreasuryModule::surplus_pool(), 200);
		assert_eq!(Assets::balance(NATIVE, &BOB), 1100);
		let ref_count_1 = frame_system::Pallet::<Runtime>::consumers(&CDPTreasuryModule::account_id());
		let bob_ref_count_1 = frame_system::Pallet::<Runtime>::consumers(&BOB);
		assert_eq!(ref_count_1, ref_count_0);
		assert_eq!(bob_ref_count_1, bob_ref_count_0);
	});
}

#[test]
fn cancel_collateral_auction_failed() {
	ExtBuilder::default().build().execute_with(|| {
		assert_ok!(<AuctionManagerModule as AuctionManager<AccountId>>::new_collateral_auction(
			&ALICE, NATIVE, 10, 100
		));

		assert_noop!(
			<AuctionManagerModule as AuctionManager<AccountId>>::cancel_auction(0),
			Error::<Runtime>::MustAfterShutdown,
		);
		mock_shutdown();

		assert_ok!(AuctionModule::bid(RuntimeOrigin::signed(ALICE), 0, 100));
		let collateral_auction = CollateralAuctions::<Runtime>::get(0).unwrap();
		assert!(!collateral_auction.always_forward());
		assert_eq!(AuctionModule::auctions(0).and_then(|a| a.bid), Some((ALICE, 100)));
		assert!(collateral_auction.in_reverse_stage(Rate::from_rational(10, 1)));
		assert_noop!(
			<AuctionManagerModule as AuctionManager<AccountId>>::cancel_auction(0),
			Error::<Runtime>::InReverseStage,
		);
	});
}

#[test]
fn cancel_collateral_auction_work() {
	ExtBuilder::default().build().execute_with(|| {
		System::set_block_number(1);
		assert_ok!(<AuctionManagerModule as AuctionManager<AccountId>>::new_collateral_auction(
			&ALICE, NATIVE, 10, 100
		));
		assert_eq!(TotalCollateralInAuction::<Runtime>::get(), 10);
		assert_eq!(TotalTargetInAuction::<Runtime>::get(), 100);
		assert_eq!(CDPTreasuryModule::surplus_pool(), 0);
		assert_eq!(CDPTreasuryModule::debit_pool(), 0);
		assert_ok!(AuctionModule::bid(RuntimeOrigin::signed(BOB), 0, 80));
		assert_eq!(Assets::balance(STABLE, &BOB), 920);
		assert_eq!(CDPTreasuryModule::surplus_pool(), 80);
		assert_eq!(CDPTreasuryModule::debit_pool(), 0);

		mock_shutdown();
		assert_ok!(<AuctionManagerModule as AuctionManager<AccountId>>::cancel_auction(0));
		System::assert_last_event(RuntimeEvent::AuctionManagerModule(Event::CancelAuction {
			auction_id: 0,
		}));

		assert_eq!(Assets::balance(STABLE, &BOB), 1000);
		assert_eq!(TotalCollateralInAuction::<Runtime>::get(), 0);
		assert_eq!(TotalTargetInAuction::<Runtime>::get(), 0);
		assert_eq!(CDPTreasuryModule::debit_pool(), 0);
		assert_eq!(CDPTreasuryModule::surplus_pool(), 0);
		assert!(!CollateralAuctions::<Runtime>::get(0).is_some());
		assert!(!AuctionModule::auctions(0).is_some());
	});
}
