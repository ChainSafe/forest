// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::convert::TryFrom;

use crate::blocks::{Block, BlockHeader, FullTipset};
use crate::libp2p::chain_exchange::{CompactedMessages, TipsetBundle};
use crate::message::SignedMessage;
use crate::shim::{
    address::Address,
    crypto::Signature,
    message::{Message, Message_v3},
};
use num::BigInt;

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
    let ua: Message = Message_v3 {
        to: Address::new_id(0).into(),
        from: Address::new_id(0).into(),
        ..Message_v3::default()
    }
    .into();
    let ub: Message = Message_v3 {
        to: Address::new_id(1).into(),
        from: Address::new_id(1).into(),
        ..Message_v3::default()
    }
    .into();
    let uc: Message = Message_v3 {
        to: Address::new_id(2).into(),
        from: Address::new_id(2).into(),
        ..Message_v3::default()
    }
    .into();
    let ud: Message = Message_v3 {
        to: Address::new_id(3).into(),
        from: Address::new_id(3).into(),
        ..Message_v3::default()
    }
    .into();
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
        // Invalidate tipset bundle by not having includes same length as number of
        // blocks
        m.secp_msg_includes = vec![vec![0]];
    }
    assert!(
        FullTipset::try_from(tsb).is_err(),
        "Invalid includes index vector should return error"
    );
}
