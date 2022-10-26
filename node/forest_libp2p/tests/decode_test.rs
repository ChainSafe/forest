// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_blocks::{Block, BlockHeader, FullTipset};
use forest_libp2p::chain_exchange::{
    ChainExchangeResponse, ChainExchangeResponseStatus, CompactedMessages, TipsetBundle,
};
use forest_message::SignedMessage;
use fvm_shared::address::Address;
use fvm_shared::crypto::signature::Signature;
use fvm_shared::{bigint::BigInt, message::Message};
use std::convert::TryFrom;

#[test]
fn convert_single_tipset_bundle() {
    let block = Block {
        header: BlockHeader::builder()
            .miner_address(Address::new_id(0))
            .build()
            .unwrap(),
        bls_messages: Vec::new(),
        secp_messages: Vec::new(),
    };
    let bundle = TipsetBundle {
        blocks: vec![block.header.clone()],
        messages: Some(CompactedMessages {
            bls_msgs: Vec::new(),
            bls_msg_includes: vec![Vec::new()],
            secp_msgs: Vec::new(),
            secp_msg_includes: vec![Vec::new()],
        }),
    };

    let res = ChainExchangeResponse {
        chain: vec![bundle],
        status: ChainExchangeResponseStatus::Success,
        message: "".into(),
    }
    .into_result::<FullTipset>()
    .unwrap();

    assert_eq!(res, [FullTipset::new(vec![block]).unwrap()]);
}

#[test]
fn tipset_bundle_to_full_tipset() {
    let h0 = BlockHeader::builder()
        .weight(BigInt::from(1u32))
        .miner_address(Address::new_id(0))
        .build()
        .unwrap();
    let h1 = BlockHeader::builder()
        .weight(BigInt::from(1u32))
        .miner_address(Address::new_id(1))
        .build()
        .unwrap();
    let ua = Message {
        to: Address::new_id(0),
        from: Address::new_id(0),
        ..Message::default()
    };
    let ub = Message {
        to: Address::new_id(1),
        from: Address::new_id(1),
        ..Message::default()
    };
    let uc = Message {
        to: Address::new_id(2),
        from: Address::new_id(2),
        ..Message::default()
    };
    let ud = Message {
        to: Address::new_id(3),
        from: Address::new_id(3),
        ..Message::default()
    };
    let sa = SignedMessage::new_unchecked(ua.clone(), Signature::new_secp256k1(vec![0]));
    let sb = SignedMessage::new_unchecked(ub.clone(), Signature::new_secp256k1(vec![0]));
    let sc = SignedMessage::new_unchecked(uc.clone(), Signature::new_secp256k1(vec![0]));
    let sd = SignedMessage::new_unchecked(ud.clone(), Signature::new_secp256k1(vec![0]));

    let b0 = Block {
        header: h0.clone(),
        secp_messages: vec![sa.clone(), sb.clone(), sd.clone()],
        bls_messages: vec![ua.clone(), ub.clone()],
    };
    let b1 = Block {
        header: h1.clone(),
        secp_messages: vec![sb.clone(), sc.clone(), sa.clone()],
        bls_messages: vec![uc.clone(), ud.clone()],
    };

    let mut tsb = TipsetBundle {
        blocks: vec![h0, h1],
        messages: Some(CompactedMessages {
            secp_msgs: vec![sa, sb, sc, sd],
            secp_msg_includes: vec![vec![0, 1, 3], vec![1, 2, 0]],
            bls_msgs: vec![ua, ub, uc, ud],
            bls_msg_includes: vec![vec![0, 1], vec![2, 3]],
        }),
    };

    assert_eq!(
        FullTipset::try_from(tsb.clone()).unwrap(),
        FullTipset::new(vec![b0, b1]).unwrap()
    );

    let mut cloned = tsb.clone();
    if let Some(m) = cloned.messages.as_mut() {
        m.secp_msg_includes = vec![vec![0, 4], vec![0]];
    }
    // Invalidate tipset bundle by having invalid index
    assert!(
        FullTipset::try_from(cloned).is_err(),
        "Invalid index should return error"
    );

    if let Some(m) = tsb.messages.as_mut() {
        // Invalidate tipset bundle by not having includes same length as number of blocks
        m.secp_msg_includes = vec![vec![0]];
    }
    assert!(
        FullTipset::try_from(tsb).is_err(),
        "Invalid includes index vector should return error"
    );
}
