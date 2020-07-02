// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "test_constructors")]

use address::Address;
use blocks::{
    Block, BlockHeader, EPostProof, EPostTicket, FullTipset, Ticket, Tipset, TipsetKeys, TxMeta,
};
use chain::TipsetMetadata;
use cid::{multihash::Blake2b256, Cid};
use crypto::{Signature, Signer, VRFProof};
use encoding::{from_slice, to_vec};
use forest_libp2p::blocksync::{BlockSyncResponse, TipsetBundle};
use message::{SignedMessage, UnsignedMessage};
use num_bigint::BigUint;
use std::error::Error;

/// Defines a TipsetKey used in testing
pub fn template_key(data: &[u8]) -> Cid {
    Cid::new_from_cbor(data, Blake2b256)
}

/// Defines a block header used in testing
fn template_header(
    ticket_p: Vec<u8>,
    cid: Cid,
    timestamp: u64,
    epoch: u64,
    msg_root: Cid,
    weight: u64,
) -> BlockHeader {
    let cids = construct_keys();
    BlockHeader::builder()
        .parents(TipsetKeys {
            cids: vec![cids[0].clone()],
        })
        .miner_address(Address::new_actor(&ticket_p))
        .timestamp(timestamp)
        .ticket(Ticket {
            vrfproof: VRFProof::new(ticket_p),
        })
        .messages(msg_root)
        .signature(Some(Signature::new_bls(vec![1, 4, 3, 6, 7, 1, 2])))
        .epoch(epoch)
        .weight(BigUint::from(weight))
        .cached_cid(cid)
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
pub fn construct_header(epoch: u64, weight: u64) -> Vec<BlockHeader> {
    let data0: Vec<u8> = vec![1, 4, 3, 6, 7, 1, 2];
    let data1: Vec<u8> = vec![1, 4, 3, 6, 1, 1, 2, 2, 4, 5, 3, 12, 2, 1];
    let data2: Vec<u8> = vec![1, 4, 3, 6, 1, 1, 2, 2, 4, 5, 3, 12, 2];
    let cids = construct_keys();
    // setup a deterministic message root within block header
    let meta = TxMeta {
        bls_message_root: Cid::from_raw_cid(
            "bafy2bzacec4insvxxjqhl4sqdfjioz3gotxjrflb3cdpd3trtvw3zvm75jdzc",
        )
        .unwrap(),
        secp_message_root: Cid::from_raw_cid(
            "bafy2bzacecbnlmwafpin7d4wmnb6sgtsdo6cfp4dhjbroq2g574eqrzc65e5a",
        )
        .unwrap(),
    };
    let bz = to_vec(&meta).unwrap();
    let msg_root = Cid::new_from_cbor(&bz, Blake2b256);

    return vec![
        template_header(data0, cids[0].clone(), 1, epoch, msg_root.clone(), weight),
        template_header(data1, cids[1].clone(), 2, epoch, msg_root.clone(), weight),
        template_header(data2, cids[2].clone(), 3, epoch, msg_root, weight),
    ];
}

/// Returns a Ticket to be used for testing
pub fn construct_ticket() -> Ticket {
    let vrf_result = VRFProof::new(base64::decode("lmRJLzDpuVA7cUELHTguK9SFf+IVOaySG8t/0IbVeHHm3VwxzSNhi1JStix7REw6Apu6rcJQV1aBBkd39gQGxP8Abzj8YXH+RdSD5RV50OJHi35f3ixR0uhkY6+G08vV").unwrap());
    Ticket::new(vrf_result)
}

/// Returns a deterministic EPostProof to be used for testing
pub fn construct_epost_proof() -> EPostProof {
    let etik = EPostTicket {
        partial: base64::decode("TFliU6/pdbjRyomejlXMS77qjYdMDty07vigvXH/vjI=").unwrap(),
        sector_id: 284,
        challenge_index: 5,
    };

    EPostProof{
        proof: from_slice(&base64::decode("rn85uiodD29xvgIuvN5/g37IXghPtVtl3li9y+nPHCueATI1q1/oOn0FEIDXRWHLpZ4CzAqOdQh9rdHih+BI5IsdI1YpwV+UdNDspJVW/cinVE+ZoiO86ap30l77RLkrEwxUZ5v8apsSRUizoXh1IFrHgK06gk1wl5LaxY2i/CQgBoWIPx9o2EYMBbNfQcu+pRzFmiDjzT6BIhYrPbo+gm6wHFiNhp3FvAuSUH2/N+5MKZo7Eh7LwgGLc0fL4MEI").unwrap()).unwrap(),
        post_rand: base64::decode("hdodcCz5kLJYRb9PT7m4z9kRvc9h02KMye9DOklnQ8v05X2ds9rgNhcTV+d/cXS+AvADHpepQODMV/6E1kbT99kdFt0xMNUsO/9YbH4ujif7sY0P8pgRAunlMgPrx7Sx").unwrap(),
        candidates: vec![etik]
    }
}

/// Returns a full block used for testing
pub fn construct_block() -> Block {
    const EPOCH: u64 = 1;
    const WEIGHT: u64 = 10;
    let headers = construct_header(EPOCH, WEIGHT);
    let (bls_messages, secp_messages) = construct_messages();

    Block {
        header: headers[0].clone(),
        secp_messages: vec![secp_messages],
        bls_messages: vec![bls_messages],
    }
}

/// Returns a tipset used for testing
pub fn construct_tipset(epoch: u64, weight: u64) -> Tipset {
    Tipset::new(construct_header(epoch, weight)).unwrap()
}

/// Returns a full tipset used for testing
pub fn construct_full_tipset() -> FullTipset {
    const EPOCH: u64 = 1;
    const WEIGHT: u64 = 10;
    let headers = construct_header(EPOCH, WEIGHT);
    let mut blocks: Vec<Block> = Vec::with_capacity(headers.len());
    let (bls_messages, secp_messages) = construct_messages();

    blocks.push(Block {
        header: headers[0].clone(),
        secp_messages: vec![secp_messages],
        bls_messages: vec![bls_messages],
    });

    FullTipset::new(blocks).unwrap()
}

/// Returns TipsetMetadata used for testing
pub fn construct_tipset_metadata() -> TipsetMetadata {
    const EPOCH: u64 = 1;
    const WEIGHT: u64 = 10;
    let tip_set = construct_tipset(EPOCH, WEIGHT);
    TipsetMetadata {
        tipset_state_root: tip_set.blocks()[0].state_root().clone(),
        tipset_receipts_root: tip_set.blocks()[0].message_receipts().clone(),
        tipset: tip_set,
    }
}

const DUMMY_SIG: [u8; 1] = [0u8];

struct DummySigner;
impl Signer for DummySigner {
    fn sign_bytes(&self, _: Vec<u8>, _: &Address) -> Result<Signature, Box<dyn Error>> {
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
pub fn construct_tipset_bundle(epoch: u64, weight: u64) -> TipsetBundle {
    let headers = construct_header(epoch, weight);
    let (bls, secp) = construct_messages();
    let includes: Vec<Vec<u64>> = (0..headers.len()).map(|_| vec![]).collect();

    TipsetBundle {
        blocks: headers,
        bls_msgs: vec![bls],
        secp_msgs: vec![secp],
        bls_msg_includes: includes.clone(),
        secp_msg_includes: includes,
    }
}

/// Returns a RPCResponse used for testing
pub fn construct_blocksync_response() -> BlockSyncResponse {
    // construct block sync response
    BlockSyncResponse {
        chain: vec![
            construct_tipset_bundle(3, 10),
            construct_tipset_bundle(2, 10),
            construct_tipset_bundle(1, 10),
        ],
        status: 0,
        message: "message".to_owned(),
    }
}
