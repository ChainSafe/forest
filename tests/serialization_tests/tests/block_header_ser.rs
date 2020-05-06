// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "serde_tests")]

use cid::{
    json::{self, CidJson},
    Cid,
};
use crypto::VRFProof;
use encoding::{from_slice, to_vec};
use forest_blocks::{BlockHeader, EPostProof, EPostTicket, Ticket, TipSetKeys};
use hex::encode;
use num_traits::FromPrimitive;
use serde::Deserialize;
use serialization_tests::SignatureVector;
use std::fs::File;
use std::io::prelude::*;
use vm::PoStProof;

#[derive(Debug, Deserialize)]
struct TicketVector {
    #[serde(alias = "VRFProof")]
    proof: String,
}

impl From<TicketVector> for Ticket {
    fn from(v: TicketVector) -> Self {
        Self {
            vrfproof: VRFProof::new(base64::decode(&v.proof).unwrap()),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ProofsVector {
    #[serde(alias = "RegisteredProof")]
    registered_proof: u8,
    #[serde(alias = "ProofBytes")]
    bytes: String,
}

impl From<ProofsVector> for PoStProof {
    fn from(v: ProofsVector) -> Self {
        Self {
            registered_proof: FromPrimitive::from_u8(v.registered_proof).unwrap(),
            proof_bytes: base64::decode(&v.bytes).unwrap(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct CandidatesVector {
    #[serde(alias = "Partial")]
    partial: String,
    #[serde(alias = "SectorID")]
    sector_id: u64,
    #[serde(alias = "ChallengeIndex")]
    challenge_index: u64,
}

impl From<CandidatesVector> for EPostTicket {
    fn from(v: CandidatesVector) -> Self {
        Self {
            partial: base64::decode(&v.partial).unwrap(),
            sector_id: v.sector_id,
            challenge_index: v.challenge_index,
        }
    }
}

#[derive(Debug, Deserialize)]
struct EPoStProofVector {
    #[serde(alias = "Proofs")]
    proofs: Vec<ProofsVector>,
    #[serde(alias = "PostRand")]
    post_rand: String,
    #[serde(alias = "Candidates")]
    candidates: Vec<CandidatesVector>,
}

impl From<EPoStProofVector> for EPostProof {
    fn from(v: EPoStProofVector) -> Self {
        Self {
            proof: v.proofs.into_iter().map(PoStProof::from).collect(),
            post_rand: base64::decode(&v.post_rand).unwrap(),
            candidates: v.candidates.into_iter().map(EPostTicket::from).collect(),
        }
    }
}

// TODO update vectors when serialization vectors submodule updated

#[derive(Deserialize)]
struct BlockVector {
    #[serde(alias = "Miner")]
    miner: String,
    #[serde(alias = "Ticket")]
    ticket: TicketVector,
    #[serde(alias = "EPostProof")]
    _e_post: EPoStProofVector,
    #[serde(alias = "Parents")]
    parents: Vec<CidJson>,
    #[serde(alias = "ParentWeight")]
    parent_weight: String,
    #[serde(alias = "Height")]
    epoch: u64,
    #[serde(alias = "ParentStateRoot", with = "json")]
    state_root: Cid,
    #[serde(alias = "ParentMessageReceipts", with = "json")]
    message_receipts: Cid,
    #[serde(alias = "Messages", with = "json")]
    messages: Cid,
    #[serde(alias = "BLSAggregate")]
    bls_agg: SignatureVector,
    #[serde(alias = "Timestamp")]
    timestamp: u64,
    #[serde(alias = "BlockSig")]
    signature: SignatureVector,
    #[serde(alias = "ForkSignaling")]
    fork_signaling: u64,
}

impl From<BlockVector> for BlockHeader {
    fn from(v: BlockVector) -> BlockHeader {
        BlockHeader::builder()
            .parents(TipSetKeys::new(
                v.parents.into_iter().map(|c| c.0).collect(),
            ))
            .weight(v.parent_weight.parse().unwrap())
            .epoch(v.epoch)
            .miner_address(v.miner.parse().unwrap())
            .messages(v.messages.into())
            .message_receipts(v.message_receipts.into())
            .state_root(v.state_root.into())
            .fork_signal(v.fork_signaling)
            .signature(Some(v.signature.into()))
            .timestamp(v.timestamp)
            .ticket(v.ticket.into())
            .bls_aggregate(Some(v.bls_agg.into()))
            .build_and_validate()
            .unwrap()
    }
}

#[derive(Deserialize)]
struct BlockHeaderVector {
    block: BlockVector,
    cbor_hex: String,
    cid: String,
}

fn encode_assert_cbor(header: &BlockHeader, expected: &str, cid: &Cid) {
    let enc_bz: Vec<u8> = to_vec(header).expect("Cbor serialization failed");

    // Assert the header is encoded in same format
    assert_eq!(encode(enc_bz.as_slice()), expected);
    // Assert decoding from those bytes goes back to unsigned header
    assert_eq!(
        &from_slice::<BlockHeader>(&enc_bz).expect("Should be able to deserialize cbor bytes"),
        header
    );
    assert_eq!(header.cid(), cid);
}

#[test]
#[ignore]
fn header_cbor_vectors() {
    let mut file = File::open("../serialization-vectors/block_headers.json").unwrap();
    let mut string = String::new();
    file.read_to_string(&mut string).unwrap();

    let vectors: Vec<BlockHeaderVector> =
        serde_json::from_str(&string).expect("Test vector deserialization failed");
    for tv in vectors {
        encode_assert_cbor(
            &BlockHeader::from(tv.block),
            &tv.cbor_hex,
            &tv.cid.parse().unwrap(),
        )
    }
}
