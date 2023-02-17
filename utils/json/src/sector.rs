// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod json {
    use base64::{prelude::BASE64_STANDARD, Engine};
    use cid::Cid;
    use forest_shim::sector::{PoStProof, RegisteredPoStProof, RegisteredSealProof, SectorInfo};
    use fvm_shared::sector::SectorNumber;
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
        #[serde(with = "crate::cid")]
        #[serde(rename = "SealedCID")]
        pub sealed_cid: Cid,
    }

    impl From<SectorInfo> for SectorInfoJson {
        fn from(sector: SectorInfo) -> Self {
            Self {
                proof: sector.proof.into(),
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
            proof_bytes: BASE64_STANDARD.encode(&m.proof_bytes),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<PoStProof, D::Error>
    where
        D: Deserializer<'de>,
    {
        let m: JsonHelper = Deserialize::deserialize(deserializer)?;
        let reg_post_proof = RegisteredPoStProof::from(m.post_proof);
        let proof_bytes = BASE64_STANDARD
            .decode(m.proof_bytes)
            .map_err(de::Error::custom)?;
        let post_proof = PoStProof::new(reg_post_proof, proof_bytes);
        Ok(post_proof)
    }

    pub mod vec {
        use forest_shim::sector::PoStProof;
        use forest_utils::json::GoVecVisitor;
        use serde::ser::SerializeSeq;

        use super::*;

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
mod tests {
    use forest_shim::{
        sector::{PoStProof, RegisteredPoStProof},
        Inner,
    };
    use quickcheck_macros::quickcheck;
    use serde_json;

    #[derive(Clone, Debug, PartialEq)]
    struct PoStProofWrapper {
        postproof: PoStProof,
    }

    type RegisteredPoStProofFVM = <RegisteredPoStProof as Inner>::FVM;

    impl quickcheck::Arbitrary for PoStProofWrapper {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let registered_postproof = g
                .choose(&[
                    RegisteredPoStProofFVM::StackedDRGWinning2KiBV1,
                    RegisteredPoStProofFVM::StackedDRGWinning8MiBV1,
                    RegisteredPoStProofFVM::StackedDRGWinning512MiBV1,
                    RegisteredPoStProofFVM::StackedDRGWinning32GiBV1,
                    RegisteredPoStProofFVM::StackedDRGWinning64GiBV1,
                    RegisteredPoStProofFVM::StackedDRGWindow2KiBV1,
                    RegisteredPoStProofFVM::StackedDRGWindow8MiBV1,
                    RegisteredPoStProofFVM::StackedDRGWindow512MiBV1,
                    RegisteredPoStProofFVM::StackedDRGWindow32GiBV1,
                    RegisteredPoStProofFVM::StackedDRGWindow64GiBV1,
                ])
                .unwrap();
            let postproof = PoStProof::new((*registered_postproof).into(), Vec::arbitrary(g));
            PoStProofWrapper { postproof }
        }
    }

    #[quickcheck]
    fn postproof_roundtrip(postproof: PoStProofWrapper) {
        let serialized: String =
            forest_test_utils::to_string_with!(&postproof.postproof, super::json::serialize);
        let parsed = forest_test_utils::from_str_with!(&serialized, super::json::deserialize);
        assert_eq!(postproof.postproof, parsed);
    }
}
