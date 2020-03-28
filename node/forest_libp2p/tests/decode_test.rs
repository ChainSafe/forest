// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crypto::{Signature, Signer};
use forest_address::Address;
use forest_blocks::{Block, BlockHeader, FullTipset};
use forest_libp2p::blocksync::{BlockSyncResponse, TipSetBundle};
use forest_message::{SignedMessage, UnsignedMessage};
use num_bigint::BigUint;
use std::convert::TryFrom;
use std::error::Error;

const DUMMY_SIG: [u8; 1] = [0u8];

/// Test struct to generate one byte signature for testing
struct DummySigner;
impl Signer for DummySigner {
    fn sign_bytes(&self, _: Vec<u8>, _: &Address) -> Result<Signature, Box<dyn Error>> {
        Ok(Signature::new_secp256k1(DUMMY_SIG.to_vec()))
    }
}

#[test]
fn convert_single_tipset_bundle() {
    let bundle = TipSetBundle {
        blocks: Vec::new(),
        bls_msgs: Vec::new(),
        bls_msg_includes: Vec::new(),
        secp_msgs: Vec::new(),
        secp_msg_includes: Vec::new(),
    };

    let res = BlockSyncResponse {
        chain: vec![bundle],
        status: 0,
        message: "".into(),
    }
    .into_result()
    .unwrap();

    assert_eq!(res, [FullTipset::new(vec![])]);
}

#[test]
fn tipset_bundle_to_full_tipset() {
    let h0 = BlockHeader::builder()
        .weight(BigUint::from(1u32))
        .build()
        .unwrap();
    let h1 = BlockHeader::builder()
        .weight(BigUint::from(2u32))
        .build()
        .unwrap();
    let ua = UnsignedMessage::builder()
        .to(Address::new_id(0).unwrap())
        .from(Address::new_id(0).unwrap())
        .build()
        .unwrap();
    let ub = UnsignedMessage::builder()
        .to(Address::new_id(1).unwrap())
        .from(Address::new_id(1).unwrap())
        .build()
        .unwrap();
    let uc = UnsignedMessage::builder()
        .to(Address::new_id(2).unwrap())
        .from(Address::new_id(2).unwrap())
        .build()
        .unwrap();
    let ud = UnsignedMessage::builder()
        .to(Address::new_id(3).unwrap())
        .from(Address::new_id(3).unwrap())
        .build()
        .unwrap();
    let sa = SignedMessage::new(&ua, &DummySigner).unwrap();
    let sb = SignedMessage::new(&ua, &DummySigner).unwrap();
    let sc = SignedMessage::new(&ua, &DummySigner).unwrap();
    let sd = SignedMessage::new(&ua, &DummySigner).unwrap();

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

    let mut tsb = TipSetBundle {
        blocks: vec![h0, h1],
        secp_msgs: vec![sa, sb, sc, sd],
        secp_msg_includes: vec![vec![0, 1, 3], vec![1, 2, 0]],
        bls_msgs: vec![ua, ub, uc, ud],
        bls_msg_includes: vec![vec![0, 1], vec![2, 3]],
    };

    assert_eq!(
        FullTipset::try_from(tsb.clone()).unwrap(),
        FullTipset::new(vec![b0, b1])
    );

    // Invalidate tipset bundle by having invalid index
    tsb.secp_msg_includes = vec![vec![0, 4], vec![0]];
    assert!(
        FullTipset::try_from(tsb.clone()).is_err(),
        "Invalid index should return error"
    );

    // Invalidate tipset bundle by not having includes same length as number of blocks
    tsb.secp_msg_includes = vec![vec![0]];
    assert!(
        FullTipset::try_from(tsb.clone()).is_err(),
        "Invalid includes index vector should return error"
    );
}
