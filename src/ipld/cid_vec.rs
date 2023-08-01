// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::utils::cid::{CidVariant, BLAKE2B256_SIZE};
use cid::{
    multihash::{self, Code::Blake2b256},
    Cid,
};
use fvm_ipld_encoding::DAG_CBOR;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// CidVec takes advantage of the fact that the V1 DAG-CBOR Blake2b-256 variant (which can be stored in 32 bytes vs 96 bytes for a `Cid` type) is +99.99% of all CIDs. CidVec defaults to the `Vec<[u8; BLAKE2B256_SIZE]>` type, only using the more expensive `Vec<Cid>` type when necessary.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CidVec {
    V1Cids(Vec<[u8; BLAKE2B256_SIZE]>),
    AllCids(Vec<Cid>),
}

impl Serialize for CidVec {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.cids().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CidVec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::from(<Vec<Cid>>::deserialize(deserializer)?))
    }
}

impl Default for CidVec {
    fn default() -> Self {
        Self::V1Cids(Vec::new())
    }
}

impl FromIterator<Cid> for CidVec {
    fn from_iter<T: IntoIterator<Item = Cid>>(iter: T) -> Self {
        let mut vec = Self::new();
        for i in iter {
            vec.push(i);
        }
        vec
    }
}

impl From<Vec<Cid>> for CidVec {
    fn from(vec: Vec<Cid>) -> Self {
        // Converts `Vec<Cid>` to `CidVec::V1Cids` if possible; otherwise, converts to `CidVec::AllCids`.
        let mut cid_vec = CidVec::new();
        for cid in vec {
            cid_vec.push(cid);
        }
        cid_vec
    }
}

impl From<&[Cid]> for CidVec {
    fn from(vec: &[Cid]) -> Self {
        vec.iter().cloned().collect()
    }
}

impl From<CidVec> for Vec<Cid> {
    fn from(vec: CidVec) -> Self {
        vec.cids()
    }
}

impl CidVec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_from_cid(cid: Cid) -> Self {
        match cid.try_into() {
            Ok(CidVariant::V1DagCborBlake2b(bytes)) => Self::V1Cids(vec![bytes]),
            _ => Self::AllCids(vec![cid]),
        }
    }

    pub fn cids(&self) -> Vec<Cid> {
        match self {
            Self::V1Cids(cids) => cids
                .iter()
                .map(|c| {
                    Cid::new_v1(
                        DAG_CBOR,
                        multihash::Multihash::wrap(Blake2b256.into(), c)
                            .expect("failed to convert digest to CID"),
                    )
                })
                .collect(),
            Self::AllCids(cids) => cids.clone(),
        }
    }

    pub fn push(&mut self, cid: Cid) {
        match self {
            Self::V1Cids(cids) => {
                if let Ok(CidVariant::V1DagCborBlake2b(bytes)) = cid.try_into() {
                    cids.push(bytes);
                } else {
                    let mut cids: Vec<Cid> = std::mem::take(cids)
                        .into_iter()
                        .map(|c| {
                            Cid::new_v1(
                                DAG_CBOR,
                                multihash::Multihash::wrap(Blake2b256.into(), &c)
                                    .expect("failed to convert digest to CID"),
                            )
                        })
                        .collect();
                    cids.push(cid);
                    *self = Self::AllCids(cids);
                }
            }
            Self::AllCids(cids) => cids.push(cid),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use cid::multihash::MultihashDigest;
    use quickcheck::Arbitrary;
    use quickcheck_macros::quickcheck;

    impl Arbitrary for CidVec {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            // Although the vast majority of CIDs are V1DagCborBlake2b, we want to generate the variants of CidVec with equal probability.
            if bool::arbitrary(g) {
                Vec::arbitrary(g).into_iter().collect()
            } else {
                // Quickcheck does not reliably generate the DAG_CBOR/Blake2b variant of V1 CIDs, but we can manually create them from an arbitrary Vec<u32>.
                let vec: Vec<u32> = Vec::arbitrary(g);
                vec.into_iter()
                    .map(|bytes| {
                        Cid::new_v1(
                            DAG_CBOR,
                            multihash::Code::Blake2b256.digest(&bytes.to_be_bytes()),
                        )
                    })
                    .collect()
            }
        }
    }

    #[quickcheck]
    fn cidvec_to_vec_of_cids_to_cidvec(cidvec: CidVec) {
        assert_eq!(cidvec, CidVec::from(Vec::<Cid>::from(cidvec.clone())));
    }

    #[quickcheck]
    fn serialize_vec_of_cids_deserialize_cidvec(vec_of_cids: Vec<Cid>) {
        let serialized = serde_json::to_string(&vec_of_cids).unwrap();
        let parsed: CidVec = serde_json::from_str(&serialized).unwrap();
        assert_eq!(vec_of_cids, Vec::<Cid>::from(parsed));
    }

    #[quickcheck]
    fn serialize_cidvec_deserialize_vec_of_cids(cidvec: CidVec) {
        let serialized = serde_json::to_string(&cidvec).unwrap();
        let parsed: Vec<Cid> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(Vec::<Cid>::from(cidvec), parsed);
    }
}
