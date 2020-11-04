// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::paych::SignedVoucher;
use address::Address;
use async_std::task;
use num_bigint::BigInt;
use paych::{ChannelInfo, PaychStore, VoucherInfo, DIR_OUTBOUND};

#[test]
fn test_store() {
    task::block_on(async {
        let mut store = PaychStore::new();
        let addrs = store.list_channels().await.unwrap();
        assert_eq!(addrs.len(), 0);

        let chan1 = Address::new_id(100);
        let chan2 = Address::new_id(200);
        let to1 = Address::new_id(101);
        let to2 = Address::new_id(201);
        let from1 = Address::new_id(102);
        let from2 = Address::new_id(202);

        let ci1 = ChannelInfo {
            id: "".to_string(),
            channel: Some(chan1.clone()),
            vouchers: vec![VoucherInfo {
                voucher: SignedVoucher {
                    channel_addr: Address::new_id(1),
                    time_lock_min: 0,
                    time_lock_max: 0,
                    secret_pre_image: vec![],
                    extra: None,
                    lane: 0,
                    nonce: 0,
                    amount: Default::default(),
                    min_settle_height: 0,
                    merges: vec![],
                    signature: None,
                },
                proof: Vec::new(),
                submitted: false,
            }],
            direction: DIR_OUTBOUND,
            next_lane: 0,
            control: from1.clone(),
            target: to1.clone(),
            create_msg: None,
            pending_amount: BigInt::default(),
            amount: BigInt::default(),
            add_funds_msg: None,
            settling: false,
        };

        let ci2 = ChannelInfo {
            id: "".to_string(),
            channel: Some(chan2.clone()),
            vouchers: vec![VoucherInfo {
                voucher: SignedVoucher {
                    channel_addr: Address::new_id(1),
                    time_lock_min: 0,
                    time_lock_max: 0,
                    secret_pre_image: vec![],
                    extra: None,
                    lane: 0,
                    nonce: 0,
                    amount: Default::default(),
                    min_settle_height: 0,
                    merges: vec![],
                    signature: None,
                },
                proof: Vec::new(),
                submitted: false,
            }],
            direction: DIR_OUTBOUND,
            next_lane: 0,
            control: from2.clone(),
            target: to2.clone(),
            create_msg: None,
            pending_amount: BigInt::default(),
            amount: BigInt::default(),
            add_funds_msg: None,
            settling: false,
        };

        // Track channels
        assert!(store.track_channel(ci1.clone()).await.is_ok());
        assert!(store.track_channel(ci2).await.is_ok());

        // make sure that tracking a channel twice throws error
        assert!(store.track_channel(ci1).await.is_err());

        let addrs = store.list_channels().await.unwrap();
        // Make sure that number of channel addresses in paychstore is 2 and that the proper
        // addresses have been saved
        assert_eq!(addrs.len(), 2);
        assert!(addrs.contains(&chan1));
        assert!(addrs.contains(&chan2));

        // Test to make sure that attempted to get vouchers for non-existent channel will error
        assert!(store
            .vouchers_for_paych(&mut Address::new_id(300))
            .await
            .is_err());

        // Allocate lane for channel
        let lane = store.allocate_lane(chan1.clone()).await.unwrap();
        assert_eq!(lane, 0);

        // Allocate lane for next channel
        let lane2 = store.allocate_lane(chan1.clone()).await.unwrap();
        assert_eq!(lane2, 1);

        //  Make sure that allocating a lane for non-existent channel will error
        assert!(store.allocate_lane(Address::new_id(300)).await.is_err())
    });
}
