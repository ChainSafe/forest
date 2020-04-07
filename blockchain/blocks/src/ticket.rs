// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crypto::VRFResult;
use encoding::{
    de::{self, Deserializer},
    ser::{self, Serializer},
    serde_bytes,
};
use serde::{Deserialize, Serialize};

/// A Ticket is a marker of a tick of the blockchain's clock.  It is the source
/// of randomness for proofs of storage and leader election.  It is generated
/// by the miner of a block using a VRF and a VDF.
#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Default)]
pub struct Ticket {
    /// A proof output by running a VRF on the VDFResult of the parent ticket
    pub vrfproof: VRFResult,
}

impl Ticket {
    /// Ticket constructor
    pub fn new(vrfproof: VRFResult) -> Self {
        Self { vrfproof }
    }
}

impl ser::Serialize for Ticket {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        [&self.vrfproof].serialize(serializer)
    }
}

impl<'de> de::Deserialize<'de> for Ticket {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let [cm]: [VRFResult; 1] = Deserialize::deserialize(deserializer)?;
        Ok(Self { vrfproof: cm })
    }
}

/// PoSt election candidates
#[derive(Clone, Debug, PartialEq, Default, Eq)]
pub struct EPostTicket {
    pub partial: Vec<u8>,
    pub sector_id: u64,
    pub challenge_index: u64,
}

/// Proof of Spacetime election proof
#[derive(Clone, Debug, PartialEq, Default, Eq)]
pub struct EPostProof {
    pub proof: Vec<u8>,
    pub post_rand: Vec<u8>,
    pub candidates: Vec<EPostTicket>,
}

impl ser::Serialize for EPostTicket {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct TupleEPostTicket<'a>(#[serde(with = "serde_bytes")] &'a [u8], &'a u64, &'a u64);
        TupleEPostTicket(&self.partial, &self.sector_id, &self.challenge_index)
            .serialize(serializer)
    }
}

impl<'de> de::Deserialize<'de> for EPostTicket {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct TupleEPostTicket(#[serde(with = "serde_bytes")] Vec<u8>, u64, u64);
        let TupleEPostTicket(partial, sector_id, challenge_index) =
            Deserialize::deserialize(deserializer)?;
        Ok(Self {
            partial,
            sector_id,
            challenge_index,
        })
    }
}

impl ser::Serialize for EPostProof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct TupleEPostProof<'a>(
            #[serde(with = "serde_bytes")] &'a [u8],
            #[serde(with = "serde_bytes")] &'a [u8],
            &'a [EPostTicket],
        );
        TupleEPostProof(&self.proof, &self.post_rand, &self.candidates).serialize(serializer)
    }
}

// Type defined outside of deserialize block because of bug with clippy
// with more than one annotated field
#[derive(Deserialize)]
struct TupleEPostProof(
    #[serde(with = "serde_bytes")] Vec<u8>,
    #[serde(with = "serde_bytes")] Vec<u8>,
    Vec<EPostTicket>,
);

impl<'de> Deserialize<'de> for EPostProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let TupleEPostProof(proof, post_rand, candidates) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            proof,
            post_rand,
            candidates,
        })
    }
}
