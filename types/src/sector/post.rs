// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{RegisteredProof, SectorNumber};
use cid::Cid;
use encoding::{serde_bytes, tuple::*};
use vm::{ActorID, Randomness};

pub type PoStRandomness = Randomness;

/// Information about a sector necessary for PoSt verification
#[derive(Debug, PartialEq, Default, Clone, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct SectorInfo {
    /// Used when sealing - needs to be mapped to PoSt registered proof when used to verify a PoSt
    pub proof: RegisteredProof,
    pub sector_number: SectorNumber,
    pub sealed_cid: Cid,
}

// TODO docs
#[derive(Debug, PartialEq, Default, Clone, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct PoStProof {
    pub registered_proof: RegisteredProof,
    // TODO revisit if can be array in future
    #[serde(with = "serde_bytes")]
    pub proof_bytes: Vec<u8>,
}

/// Information needed to verify a Winning PoSt attached to a block header.
/// Note: this is not used within the state machine, but by the consensus/election mechanisms.
#[derive(Debug, PartialEq, Default, Clone, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct WinningPoStVerifyInfo {
    pub randomness: PoStRandomness,
    pub proofs: Vec<PoStProof>,
    pub challenge_sectors: Vec<SectorInfo>,
    /// Used to derive 32-byte prover ID
    pub prover: ActorID,
}

/// Information needed to verify a Window PoSt submitted directly to a miner actor.
#[derive(Debug, PartialEq, Default, Clone, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct WindowPoStVerifyInfo {
    pub randomness: PoStRandomness,
    pub proofs: Vec<PoStProof>,
    pub challenged_sectors: Vec<SectorInfo>,
    pub prover: ActorID,
}

/// Information submitted by a miner to provide a Window PoSt.
#[derive(Debug, PartialEq, Default, Clone, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct OnChainWindowPoStVerifyInfo {
    pub proofs: Vec<PoStProof>,
}

#[cfg(feature = "json")]
pub mod json {
    use super::*;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    /// Wrapper for serializing a PoStProof to JSON.
    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct PoStProofJson(#[serde(with = "self")] pub PoStProof);

    /// Wrapper for serializing a PoStProof reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct PoStProofJsonRef<'a>(#[serde(with = "self")] pub &'a PoStProof);

    impl From<PoStProofJson> for PoStProof {
        fn from(wrapper: PoStProofJson) -> Self {
            wrapper.0
        }
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct JsonHelper {
        registered_proof: u8,
        proof_bytes: String,
    }

    pub fn serialize<S>(m: &PoStProof, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            registered_proof: m.registered_proof as u8,
            proof_bytes: base64::encode(&m.proof_bytes),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<PoStProof, D::Error>
    where
        D: Deserializer<'de>,
    {
        let m: JsonHelper = Deserialize::deserialize(deserializer)?;
        Ok(PoStProof {
            registered_proof: RegisteredProof::from_byte(m.registered_proof).unwrap(),
            proof_bytes: base64::decode(m.proof_bytes).map_err(de::Error::custom)?,
        })
    }

    pub mod vec {
        use super::*;
        use forest_json_utils::GoVecVisitor;
        use serde::ser::SerializeSeq;

        pub fn serialize<S>(m: &[PoStProof], serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut seq = serializer.serialize_seq(Some(m.len()))?;
            for e in m {
                seq.serialize_element(&PoStProofJsonRef(e))?;
            }
            seq.end()
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<PoStProof>, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(GoVecVisitor::<PoStProof, PoStProofJson>::new())
        }
    }
}
