// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crypto::VRFProof;
use encoding::{BytesDe, BytesSer};
use fil_types::PoStProof;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A Ticket is a marker of a tick of the blockchain's clock.  It is the source
/// of randomness for proofs of storage and leader election.  It is generated
/// by the miner of a block using a VRF and a VDF.
#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Default, Ord)]
pub struct Ticket {
    /// A proof output by running a VRF on the VDFResult of the parent ticket
    pub vrfproof: VRFProof,
}

impl Ticket {
    /// Ticket constructor
    pub fn new(vrfproof: VRFProof) -> Self {
        Self { vrfproof }
    }
}

impl Serialize for Ticket {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        [&self.vrfproof].serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Ticket {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let [cm]: [VRFProof; 1] = Deserialize::deserialize(deserializer)?;
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
    pub proof: Vec<PoStProof>,
    pub post_rand: Vec<u8>,
    pub candidates: Vec<EPostTicket>,
}

impl Serialize for EPostTicket {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            BytesSer(&self.partial),
            &self.sector_id,
            &self.challenge_index,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EPostTicket {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (BytesDe(partial), sector_id, challenge_index) =
            Deserialize::deserialize(deserializer)?;
        Ok(Self {
            partial,
            sector_id,
            challenge_index,
        })
    }
}

impl Serialize for EPostProof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.proof, BytesSer(&self.post_rand), &self.candidates).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EPostProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (proof, BytesDe(post_rand), candidates) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            proof,
            post_rand,
            candidates,
        })
    }
}
