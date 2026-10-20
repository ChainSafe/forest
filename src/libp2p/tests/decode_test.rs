// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::convert::TryFrom;

use crate::blocks::RawBlockHeader;
use crate::blocks::{Block, CachingBlockHeader, FullTipset};
use crate::libp2p::chain_exchange::{
    ChainExchangeResponse, ChainExchangeResponseStatus, CompactedMessages, TipsetBundle,
};
use crate::message::SignedMessage;
use crate::shim::{
    address::Address,
    crypto::Signature,
    message::{Message, Message_v3},
};
use crate::test_utils::dummy_ticket;
use num::BigInt;
use std::sync::Arc;

#[test]
fn convert_single_tipset_bundle() {
    let block = Block {
        header: CachingBlockHeader::new(RawBlockHeader {
            miner_address: Address::new_id(0),
            ticket: dummy_ticket(0),
            ..Default::default()
        }),
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
    let h0 = CachingBlockHeader::new(RawBlockHeader {
        miner_address: Address::new_id(0),
        ticket: dummy_ticket(0),
        weight: BigInt::from(1u32),
        ..Default::default()
    });
    let h1 = CachingBlockHeader::new(RawBlockHeader {
        miner_address: Address::new_id(1),
        ticket: dummy_ticket(1),
        weight: BigInt::from(1u32),
        ..Default::default()
    });
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
        secp_messages: vec![
            Arc::new(sa.clone()),
            Arc::new(sb.clone()),
            Arc::new(sd.clone()),
        ],
        bls_messages: vec![Arc::new(ua.clone()), Arc::new(ub.clone())],
    };
    let b1 = Block {
        header: h1.clone(),
        secp_messages: vec![
            Arc::new(sb.clone()),
            Arc::new(sc.clone()),
            Arc::new(sa.clone()),
        ],
        bls_messages: vec![Arc::new(uc.clone()), Arc::new(ud.clone())],
    };

    let mut tsb = TipsetBundle {
        blocks: vec![h0, h1],
        messages: Some(CompactedMessages {
            secp_msgs: vec![Arc::new(sa), Arc::new(sb), Arc::new(sc), Arc::new(sd)],
            secp_msg_includes: vec![vec![0, 1, 3], vec![1, 2, 0]],
            bls_msgs: vec![Arc::new(ua), Arc::new(ub), Arc::new(uc), Arc::new(ud)],
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

// An include list that repeats one index shares a single allocation instead of deep-cloning the
// message per occurrence.
#[test]
fn repeated_include_index_shares_one_allocation() {
    use fvm_ipld_encoding::RawBytes;

    let header = CachingBlockHeader::new(RawBlockHeader {
        miner_address: Address::new_id(0),
        ticket: dummy_ticket(0),
        ..Default::default()
    });
    let big: Message = Message_v3 {
        to: Address::new_id(0).into(),
        from: Address::new_id(0).into(),
        params: RawBytes::new(vec![0u8; 64 * 1024]),
        ..Message_v3::default()
    }
    .into();

    let repeats = 10_000;
    let bundle = TipsetBundle {
        blocks: vec![header],
        messages: Some(CompactedMessages {
            bls_msgs: vec![Arc::new(big)],
            bls_msg_includes: vec![vec![0u64; repeats]],
            secp_msgs: Vec::new(),
            secp_msg_includes: vec![Vec::new()],
        }),
    };

    let fts = FullTipset::try_from(bundle).unwrap();
    let msgs = &fts.blocks().first().bls_messages;
    assert_eq!(msgs.len(), repeats);
    let first = msgs.first().unwrap();
    assert!(
        msgs.iter().all(|m| Arc::ptr_eq(m, first)),
        "repeated include indexes must share one allocation, not deep-clone"
    );
}
