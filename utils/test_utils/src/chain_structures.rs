// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "test_constructors")]

use address::Address;
use blocks::{Block, BlockHeader, FullTipset, Ticket, Tipset, TipsetKeys, TxMeta};
use cid::{Cid, Code::Blake2b256};
use crypto::{Signature, Signer, VRFProof};
use encoding::to_vec;
use forest_libp2p::chain_exchange::{
    ChainExchangeResponse, ChainExchangeResponseStatus, CompactedMessages, TipsetBundle,
};
use message::{SignedMessage, UnsignedMessage};
use num_bigint::BigInt;
use std::convert::TryFrom;
use std::error::Error;

/// Defines a TipsetKey used in testing
pub fn template_key(data: &[u8]) -> Cid {
    cid::new_from_cbor(data, Blake2b256)
}

/// Defines a block header used in testing
fn template_header(
    ticket_p: Vec<u8>,
    timestamp: u64,
    epoch: i64,
    msg_root: Cid,
    weight: u64,
) -> BlockHeader {
    let cids = construct_keys();
    BlockHeader::builder()
        .parents(TipsetKeys {
            cids: vec![cids[0]],
        })
        .miner_address(Address::new_actor(&ticket_p))
        .timestamp(timestamp)
        .ticket(Some(Ticket {
            vrfproof: VRFProof::new(ticket_p),
        }))
        .messages(msg_root)
        .signature(Some(Signature::new_bls(vec![1, 4, 3, 6, 7, 1, 2])))
        .epoch(epoch)
        .weight(BigInt::from(weight))
        .build()
        .unwrap()
}

/// Returns a vec of 4 distinct CIDs
pub fn construct_keys() -> Vec<Cid> {
    return vec![
        template_key(b"test content"),
        template_key(b"awesome test content "),
        template_key(b"even better test content"),
        template_key(b"the best test content out there"),
    ];
}

/// Returns a vec of block headers to be used for testing purposes
pub fn construct_headers(epoch: i64, weight: u64) -> Vec<BlockHeader> {
    let data0: Vec<u8> = vec![1, 4, 3, 6, 7, 1, 2];
    let data1: Vec<u8> = vec![1, 4, 3, 6, 1, 1, 2, 2, 4, 5, 3, 12, 2, 1];
    let data2: Vec<u8> = vec![1, 4, 3, 6, 1, 1, 2, 2, 4, 5, 3, 12, 2];
    // setup a deterministic message root within block header
    let meta = TxMeta {
        bls_message_root: Cid::try_from(
            "bafy2bzacec4insvxxjqhl4sqdfjioz3gotxjrflb3cdpd3trtvw3zvm75jdzc",
        )
        .unwrap(),
        secp_message_root: Cid::try_from(
            "bafy2bzacecbnlmwafpin7d4wmnb6sgtsdo6cfp4dhjbroq2g574eqrzc65e5a",
        )
        .unwrap(),
    };
    let bz = to_vec(&meta).unwrap();
    let msg_root = cid::new_from_cbor(&bz, Blake2b256);

    return vec![
        template_header(data0, 1, epoch, msg_root, weight),
        template_header(data1, 2, epoch, msg_root, weight),
        template_header(data2, 3, epoch, msg_root, weight),
    ];
}

/// Returns a Ticket to be used for testing
pub fn construct_ticket() -> Ticket {
    let vrf_result = VRFProof::new(base64::decode("lmRJLzDpuVA7cUELHTguK9SFf+IVOaySG8t/0IbVeHHm3VwxzSNhi1JStix7REw6Apu6rcJQV1aBBkd39gQGxP8Abzj8YXH+RdSD5RV50OJHi35f3ixR0uhkY6+G08vV").unwrap());
    Ticket::new(vrf_result)
}

/// Returns a full block used for testing
pub fn construct_block() -> Block {
    const EPOCH: i64 = 1;
    const WEIGHT: u64 = 10;
    let headers = construct_headers(EPOCH, WEIGHT);
    let (bls_messages, secp_messages) = construct_messages();

    Block {
        header: headers[0].clone(),
        secp_messages: vec![secp_messages],
        bls_messages: vec![bls_messages],
    }
}

/// Returns a tipset used for testing
pub fn construct_tipset(epoch: i64, weight: u64) -> Tipset {
    Tipset::new(construct_headers(epoch, weight)).unwrap()
}

/// Returns a full tipset used for testing
pub fn construct_full_tipset() -> FullTipset {
    const EPOCH: i64 = 1;
    const WEIGHT: u64 = 10;
    let headers = construct_headers(EPOCH, WEIGHT);
    let mut blocks: Vec<Block> = Vec::with_capacity(headers.len());
    let (bls_messages, secp_messages) = construct_messages();

    blocks.push(Block {
        header: headers[0].clone(),
        secp_messages: vec![secp_messages],
        bls_messages: vec![bls_messages],
    });

    FullTipset::new(blocks).unwrap()
}

const DUMMY_SIG: [u8; 1] = [0u8];

struct DummySigner;
impl Signer for DummySigner {
    fn sign_bytes(&self, _: &[u8], _: &Address) -> Result<Signature, Box<dyn Error>> {
        Ok(Signature::new_secp256k1(DUMMY_SIG.to_vec()))
    }
}

/// Returns a tuple of unsigned and signed messages used for testing
pub fn construct_messages() -> (UnsignedMessage, SignedMessage) {
    let bls_messages = UnsignedMessage::builder()
        .to(Address::new_id(1))
        .from(Address::new_id(2))
        .build()
        .unwrap();

    let secp_messages = SignedMessage::new(bls_messages.clone(), &DummySigner).unwrap();
    (bls_messages, secp_messages)
}

/// Returns a TipsetBundle used for testing
pub fn construct_tipset_bundle(epoch: i64, weight: u64) -> TipsetBundle {
    let headers = construct_headers(epoch, weight);
    let (bls, secp) = construct_messages();
    let includes: Vec<Vec<u64>> = (0..headers.len()).map(|_| Vec::new()).collect();

    TipsetBundle {
        blocks: headers,
        messages: Some(CompactedMessages {
            bls_msgs: vec![bls],
            secp_msgs: vec![secp],
            bls_msg_includes: includes.clone(),
            secp_msg_includes: includes,
        }),
    }
}

pub fn construct_dummy_header() -> BlockHeader {
    BlockHeader::builder()
        .miner_address(Address::new_id(1000))
        .messages(cid::new_from_cbor(&[1, 2, 3], Blake2b256))
        .message_receipts(cid::new_from_cbor(&[1, 2, 3], Blake2b256))
        .state_root(cid::new_from_cbor(&[1, 2, 3], Blake2b256))
        .build()
        .unwrap()
}

/// Returns a RPCResponse used for testing
pub fn construct_chain_exchange_response() -> ChainExchangeResponse {
    // construct block sync response
    ChainExchangeResponse {
        chain: vec![
            construct_tipset_bundle(3, 10),
            construct_tipset_bundle(2, 10),
            construct_tipset_bundle(1, 10),
        ],
        status: ChainExchangeResponseStatus::Success,
        message: "message".to_owned(),
    }
}
