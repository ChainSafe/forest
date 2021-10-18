// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{ActorID, Randomness, RegisteredPoStProof, RegisteredSealProof, SectorNumber};
use cid::Cid;
use encoding::{serde_bytes, tuple::*};

/// Randomness type used for generating PoSt proof randomness.
pub type PoStRandomness = Randomness;

/// Information about a sector necessary for PoSt verification
#[derive(Debug, PartialEq, Clone, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct SectorInfo {
    /// Used when sealing - needs to be mapped to PoSt registered proof when used to verify a PoSt
    pub proof: RegisteredSealProof,
    pub sector_number: SectorNumber,
    pub sealed_cid: Cid,
}

/// Proof of spacetime data stored on chain.
#[derive(Debug, PartialEq, Clone, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct PoStProof {
    pub post_proof: RegisteredPoStProof,
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

    #[derive(Clone, Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct SectorInfoJson {
        #[serde(rename = "SealProof")]
        pub proof: RegisteredSealProof,
        pub sector_number: SectorNumber,
        #[serde(with = "cid::json")]
        #[serde(rename = "SealedCID")]
        pub sealed_cid: Cid,
    }

    impl From<SectorInfo> for SectorInfoJson {
        fn from(sector: SectorInfo) -> Self {
            Self {
                proof: sector.proof,
                sector_number: sector.sector_number,
                sealed_cid: sector.sealed_cid,
            }
        }
    }

    impl From<PoStProofJson> for PoStProof {
        fn from(wrapper: PoStProofJson) -> Self {
            wrapper.0
        }
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct JsonHelper {
        #[serde(rename = "PoStProof")]
        post_proof: i64,
        proof_bytes: String,
    }

    pub fn serialize<S>(m: &PoStProof, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            post_proof: i64::from(m.post_proof),
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
            post_proof: RegisteredPoStProof::from(m.post_proof),
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
