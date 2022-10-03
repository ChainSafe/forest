// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared::randomness::Randomness;
#[cfg(test)]
use fvm_shared::sector::{PoStProof, RegisteredPoStProof};

/// Randomness type used for generating PoSt proof randomness.
pub type PoStRandomness = Randomness;

pub mod json {
    use crate::{PoStProof, RegisteredPoStProof, RegisteredSealProof, SectorInfo, SectorNumber};
    use cid::Cid;
    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    /// Wrapper for serializing a `PoStProof` to JSON.
    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct PoStProofJson(#[serde(with = "self")] pub PoStProof);

    /// Wrapper for serializing a `PoStProof` reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct PoStProofJsonRef<'a>(#[serde(with = "self")] pub &'a PoStProof);

    #[derive(Clone, Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct SectorInfoJson {
        #[serde(rename = "SealProof")]
        pub proof: RegisteredSealProof,
        pub sector_number: SectorNumber,
        #[serde(with = "forest_json::cid")]
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

#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
struct PoStProofWrapper {
    postproof: PoStProof,
}

#[cfg(test)]
impl quickcheck::Arbitrary for PoStProofWrapper {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let registered_postproof = g
            .choose(&[
                RegisteredPoStProof::StackedDRGWinning2KiBV1,
                RegisteredPoStProof::StackedDRGWinning8MiBV1,
                RegisteredPoStProof::StackedDRGWinning512MiBV1,
                RegisteredPoStProof::StackedDRGWinning32GiBV1,
                RegisteredPoStProof::StackedDRGWinning64GiBV1,
                RegisteredPoStProof::StackedDRGWindow2KiBV1,
                RegisteredPoStProof::StackedDRGWindow8MiBV1,
                RegisteredPoStProof::StackedDRGWindow512MiBV1,
                RegisteredPoStProof::StackedDRGWindow32GiBV1,
                RegisteredPoStProof::StackedDRGWindow64GiBV1,
            ])
            .unwrap();
        let postproof = PoStProof {
            post_proof: *registered_postproof,
            proof_bytes: Vec::arbitrary(g),
        };
        PoStProofWrapper { postproof }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;
    use serde_json;

    macro_rules! to_string_with {
        ($obj:expr, $serializer:path) => {{
            let mut writer = Vec::new();
            $serializer($obj, &mut serde_json::ser::Serializer::new(&mut writer)).unwrap();
            String::from_utf8(writer).unwrap()
        }};
    }

    macro_rules! from_str_with {
        ($str:expr, $deserializer:path) => {
            $deserializer(&mut serde_json::de::Deserializer::from_str($str)).unwrap()
        };
    }

    #[quickcheck]
    fn postproof_roundtrip(postproof: PoStProofWrapper) {
        let serialized: String = to_string_with!(&postproof.postproof, json::serialize);
        let parsed = from_str_with!(&serialized, json::deserialize);
        assert_eq!(postproof.postproof, parsed);
    }
}
