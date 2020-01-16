// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

use super::{BlockHeader, RawBlock};
use cid::Cid;
use encoding::{Cbor, Error as EncodingError};
use message::{SignedMessage, UnsignedMessage};
use multihash::Hash;
use serde::{Deserialize, Serialize};
use std::fmt;

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
    header: BlockHeader,
    bls_messages: UnsignedMessage,
    secp_messages: SignedMessage,
}

// TODO verify format or implement custom serialize/deserialize function (if necessary):
// https://github.com/ChainSafe/ferret/issues/143

impl Cbor for Block {}

impl RawBlock for Block {
    /// returns the block raw contents as a byte array
    fn raw_data(&self) -> Result<Vec<u8>, EncodingError> {
        // TODO should serialize block header using CBOR encoding
        self.marshal_cbor()
    }
    /// returns the content identifier of the block
    fn cid(&self) -> Cid {
        self.header.cid().clone()
    }
    /// returns the hash contained in the block CID
    fn multihash(&self) -> Hash {
        self.cid().prefix().mh_type
    }
}

/// human-readable string representation of a block CID
impl fmt::Display for Block {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "block: {:?}", self.cid())
    }
}

/// Tracks the merkleroots of both secp and bls messages separately
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct TxMeta {
    pub bls_messages: Cid,
    pub secp_messages: Cid,
}

// TODO verify format or implement custom serialize/deserialize function (if necessary):
// https://github.com/ChainSafe/ferret/issues/143

/// ElectionPoStVerifyInfo seems to be connected to VRF
/// see https://github.com/filecoin-project/lotus/blob/master/chain/sync.go#L1099
struct ElectionPoStVerifyInfo {
    candidates: PoStCandidate,
    randomness: PoStRandomness,
    proof: PoStProof,
    messages: Vec<UnsignedMessage>,
}
