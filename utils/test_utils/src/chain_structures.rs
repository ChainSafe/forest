// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use blocks::{Block, BlockHeader, FullTipset, Ticket, TipSetKeys, Tipset};
use cid::{multihash::Blake2b256, Cid};
use crypto::{Signature, Signer, VRFResult};
use forest_libp2p::blocksync::TipSetBundle;
use message::{SignedMessage, UnsignedMessage};
use num_bigint::BigUint;
use std::error::Error;

const WEIGHT: u64 = 1;

/// Defines a TipsetKey used in testing
pub fn template_key(data: &[u8]) -> Cid {
    Cid::new_from_cbor(data, Blake2b256).unwrap()
}

// Defines a block header used in testing
fn template_header(ticket_p: Vec<u8>, cid: Cid, timestamp: u64, epoch: u64) -> BlockHeader {
    let cids = key_setup();
    BlockHeader::builder()
        .parents(TipSetKeys {
            cids: vec![cids[0].clone()],
        })
        .miner_address(Address::new_secp256k1(&ticket_p).unwrap())
        .timestamp(timestamp)
        .ticket(Ticket {
            vrfproof: VRFResult::new(ticket_p),
        })
        .messages(
            Cid::from_raw_cid("bafy2bzaced5inutkibck2wagtnggbvjpbr65ghdncivs3gpagx67s3xs3i5wa")
                .unwrap(),
        )
        .epoch(epoch)
        .weight(BigUint::from(WEIGHT))
        .cached_cid(cid)
        .build()
        .unwrap()
}

/// Returns a vec of distinct CIDs
pub fn key_setup() -> Vec<Cid> {
    return vec![template_key(b"test content")];
}

/// Returns a vec of block headers to be used for testing purposes
pub fn header_setup(epoch: u64) -> Vec<BlockHeader> {
    let data0: Vec<u8> = vec![1, 4, 3, 6, 7, 1, 2];
    let cids = key_setup();
    return vec![template_header(data0, cids[0].clone(), 1, epoch)];
}

/// Returns a full block used for testing
pub fn block_setup() -> Block {
    let epoch: u64 = 1;
    let headers = header_setup(epoch);
    let (bls_messages, secp_messages) = block_msgs_setup();

    Block {
        header: headers[0].clone(),
        secp_messages: vec![secp_messages],
        bls_messages: vec![bls_messages],
    }
}
/// Returns a tipset used for testing
pub fn tipset_setup(epoch: u64) -> Tipset {
    Tipset::new(header_setup(epoch)).expect("tipset is invalid")
}
/// Returns a full tipset used for testing
pub fn full_tipset_setup() -> FullTipset {
    let epoch: u64 = 1;
    let headers = header_setup(epoch);
    let mut blocks: Vec<Block> = Vec::with_capacity(headers.len());
    let (bls_messages, secp_messages) = block_msgs_setup();

    for header in headers {
        blocks.push(Block {
            header,
            secp_messages: vec![secp_messages.clone()],
            bls_messages: vec![bls_messages.clone()],
        });
    }
    FullTipset::new(blocks)
}

const DUMMY_SIG: [u8; 1] = [0u8];

struct DummySigner;
impl Signer for DummySigner {
    fn sign_bytes(&self, _: Vec<u8>, _: &Address) -> Result<Signature, Box<dyn Error>> {
        Ok(Signature::new_secp256k1(DUMMY_SIG.to_vec()))
    }
}
/// Returns a tuple of unsigned and signed messages used for testing
pub fn block_msgs_setup() -> (UnsignedMessage, SignedMessage) {
    let bls_messages = UnsignedMessage::builder()
        .to(Address::new_id(1).unwrap())
        .from(Address::new_id(2).unwrap())
        .build()
        .unwrap();

    let secp_messages = SignedMessage::new(&bls_messages, &DummySigner).unwrap();
    (bls_messages, secp_messages)
}

/// Returns a TipsetBundle used for testing
pub fn tipset_bundle(epoch: u64) -> TipSetBundle {
    let headers = header_setup(epoch);
    let (bls, secp) = block_msgs_setup();
    let includes: Vec<Vec<u64>> = (0..headers.len()).map(|_| vec![]).collect();

    TipSetBundle {
        blocks: headers,
        bls_msgs: vec![bls],
        secp_msgs: vec![secp],
        bls_msg_includes: includes.clone(),
        secp_msg_includes: includes,
    }
}
