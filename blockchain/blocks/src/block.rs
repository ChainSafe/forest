// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(dead_code)]

use super::BlockHeader;
use cid::Cid;
use encoding::Cbor;
use message::{SignedMessage, UnsignedMessage};
use multihash::Hash;
use serde::{Deserialize, Serialize};

// DefaultHashFunction represents the default hashing function to use
// TODO SHOULD BE BLAKE2B256 (256 hashing not implemented)
const DEFAULT_HASH_FUNCTION: Hash = Hash::Blake2b512;
// TODO determine the purpose for these structures, currently spec includes them but with no definition
struct ChallengeTicketsCommitment {}
struct PoStCandidate {}
struct PoStRandomness {}
struct PoStProof {}

/// A complete block
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub header: BlockHeader,
    pub bls_messages: Vec<UnsignedMessage>,
    pub secp_messages: Vec<SignedMessage>,
}

impl Block {
    /// Returns reference to BlockHeader
    pub fn to_header(&self) -> &BlockHeader {
        &self.header
    }
    pub fn bls_msgs(&self) -> &Vec<UnsignedMessage> {
        &self.bls_messages
    }
    pub fn secp_msgs(&self) -> &Vec<SignedMessage> {
        &self.secp_messages
    }
}

// TODO verify format or implement custom serialize/deserialize function (if necessary):
// https://github.com/ChainSafe/forest/issues/143

impl Cbor for Block {}

/// Tracks the merkleroots of both secp and bls messages separately
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct TxMeta {
    pub bls_messages: Cid,
    pub secp_messages: Cid,
}

// TODO verify format or implement custom serialize/deserialize function (if necessary):
// https://github.com/ChainSafe/forest/issues/143

/// ElectionPoStVerifyInfo seems to be connected to VRF
/// see https://github.com/filecoin-project/lotus/blob/master/chain/sync.go#L1099
struct ElectionPoStVerifyInfo {
    candidates: PoStCandidate,
    randomness: PoStRandomness,
    proof: PoStProof,
    messages: Vec<UnsignedMessage>,
}
