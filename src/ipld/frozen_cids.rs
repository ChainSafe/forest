// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::utils::cid::{CidVariant, BLAKE2B256_SIZE};
use cid::{
    multihash::{self, Code::Blake2b256},
    Cid,
};
use fvm_ipld_encoding::DAG_CBOR;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// `FrozenCids` takes advantage of the fact that the V1 DAG-CBOR Blake2b-256 variant 
// (which can be stored in 32 bytes vs 96 bytes for a `Cid` type) is +99.99% of 
// all CIDs. `FrozenCids` defaults to the `Box<[u8; BLAKE2B256_SIZE]>` variant of 
// `CidBox`, only using the more expensive `Box<Cid>` variant when necessary.
// The Box type has been chosen to make `FrozenCids` explicitly immutable.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FrozenCids(CidBox);

impl Default for FrozenCids {
    fn default() -> Self {
        Self(CidBox::V1DagCborBlake2bCids(Box::new([])))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum CidBox {
    V1DagCborBlake2bCids(Box<[[u8; BLAKE2B256_SIZE]]>),
    AllCids(Box<[Cid]>),
}

pub struct FrozenCidsIterator<'a> {
    buffer: &'a FrozenCids,
    current_ix: usize,
}

impl<'a> IntoIterator for &'a FrozenCids {
    type Item = Cid;
    type IntoIter = FrozenCidsIterator<'a>;
    fn into_iter(self) -> Self::IntoIter {
        FrozenCidsIterator {
            buffer: self,
            current_ix: 0,
        }
    }
}

impl Iterator for FrozenCidsIterator<'_> {
    type Item = Cid;
    fn next(&mut self) -> Option<Self::Item> {
        match &self.buffer.0 {
            CidBox::V1DagCborBlake2bCids(cids) => {
                if self.current_ix >= cids.len() {
                    None
                } else {
                    let cid = Cid::new_v1(
                        DAG_CBOR,
                        multihash::Multihash::wrap(Blake2b256.into(), &cids[self.current_ix])
                            .expect("failed to convert Blake2b digest to V1 DAG-CBOR Blake2b CID"),
                    );
                    self.current_ix += 1;
                    Some(cid)
                }
            }
            CidBox::AllCids(cids) => {
                if self.current_ix >= cids.len() {
                    None
                } else {
                    let cid = cids[self.current_ix];
                    self.current_ix += 1;
                    Some(cid)
                }
            }
        }
    }
}

impl Serialize for FrozenCids {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Vec::<Cid>::from(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FrozenCids {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::from(<Vec<Cid>>::deserialize(deserializer)?))
    }
}

impl FromIterator<Cid> for FrozenCids {
    fn from_iter<T: IntoIterator<Item = Cid>>(iter: T) -> Self {
        let mut vec = Vec::new();
        for i in iter {
            vec.push(i);
        }
        FrozenCids::from(vec)
    }
}

 // Converts `Vec<Cid>` to `FrozenCids(CidBox::V1DagCborBlake2bCids)` if possible; otherwise, converts to `FrozenCids(CidBox::AllCids)`.
impl From<Vec<Cid>> for FrozenCids {
    fn from(cids: Vec<Cid>) -> Self {
        let mut v1dagcborblake2bcids = Vec::new();
        let mut allcids = Vec::new();
        for cid in cids {
            match cid.try_into() {
                Ok(CidVariant::V1DagCborBlake2b(bytes)) => {
                    v1dagcborblake2bcids.push(bytes);
                }
                _ => {
                    allcids.push(cid);
                }
            }
        }
        if allcids.is_empty() {
            FrozenCids(CidBox::V1DagCborBlake2bCids(
                v1dagcborblake2bcids.into_boxed_slice(),
            ))
        } else {
            allcids.extend(v1dagcborblake2bcids.into_iter().map(|bytes| {
                Cid::new_v1(
                    DAG_CBOR,
                    multihash::Multihash::wrap(Blake2b256.into(), &bytes)
                        .expect("failed to convert Blake2b digest to V1 DAG-CBOR Blake2b CID"),
                )
            }));
            FrozenCids(CidBox::AllCids(allcids.into_boxed_slice()))
        }
    }
}

impl From<FrozenCids> for Vec<Cid> {
    fn from(frozen_cids: FrozenCids) -> Self {
        match frozen_cids.0 {
            CidBox::V1DagCborBlake2bCids(cids) => cids
                .iter()
                .map(|bytes| {
                    Cid::new_v1(
                        DAG_CBOR,
                        multihash::Multihash::wrap(Blake2b256.into(), bytes)
                            .expect("failed to convert Blake2b digest to V1 DAG-CBOR Blake2b CID"),
                    )
                })
                .collect(),
            CidBox::AllCids(cids) => cids.to_vec(),
        }
    }
}

impl From<&FrozenCids> for Vec<Cid> {
    fn from(frozen_cids: &FrozenCids) -> Self {
        match &frozen_cids.0 {
            CidBox::V1DagCborBlake2bCids(cids) => cids
                .iter()
                .map(|bytes| {
                    Cid::new_v1(
                        DAG_CBOR,
                        multihash::Multihash::wrap(Blake2b256.into(), bytes)
                            .expect("failed to convert Blake2b digest to V1 DAG-CBOR Blake2b CID"),
                    )
                })
                .collect(),
            CidBox::AllCids(cids) => cids.to_vec(),
        }
    }
}

impl FrozenCids {
     /// Adds a CID to `FrozenCids`, returning the appropriate `FrozenCids` variant via the `FrozenCids::from` call.
    pub fn push(&self, cid: Cid) -> Self {
        let mut cids = Vec::<Cid>::from(self);
        cids.push(cid);
        FrozenCids::from(cids)
    }

    pub fn is_empty(&self) -> bool {
        match &self.0 {
            CidBox::V1DagCborBlake2bCids(cids) => cids.is_empty(),
            CidBox::AllCids(cids) => cids.is_empty(),
        }
    }

    pub fn contains(&self, cid: Cid) -> bool {
        match &self.0 {
            CidBox::V1DagCborBlake2bCids(cids) => {
                if let Ok(CidVariant::V1DagCborBlake2b(bytes)) = cid.try_into() {
                    cids.contains(&bytes)
                } else {
                    false
                }
            }
            CidBox::AllCids(cids) => cids.contains(&cid),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use cid::multihash::MultihashDigest;
    use quickcheck::Arbitrary;
    use quickcheck_macros::quickcheck;

    impl Arbitrary for FrozenCids {
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
    fn cidvec_to_vec_of_cids_to_cidvec(cidvec: FrozenCids) {
        assert_eq!(cidvec, FrozenCids::from(Vec::<Cid>::from(cidvec.clone())));
    }

    #[quickcheck]
    fn serialize_vec_of_cids_deserialize_cidvec(vec_of_cids: Vec<Cid>) {
        let serialized = serde_json::to_string(&vec_of_cids).unwrap();
        let parsed: FrozenCids = serde_json::from_str(&serialized).unwrap();
        assert_eq!(vec_of_cids, Vec::<Cid>::from(parsed));
    }

    #[quickcheck]
    fn serialize_cidvec_deserialize_vec_of_cids(cidvec: FrozenCids) {
        let serialized = serde_json::to_string(&cidvec).unwrap();
        let parsed: Vec<Cid> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(Vec::<Cid>::from(cidvec), parsed);
    }
}
