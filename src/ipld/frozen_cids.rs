// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::utils::cid::{CidVariant, BLAKE2B256_SIZE};
use cid::{
    multihash::{self, Code::Blake2b256},
    Cid,
};
use fvm_ipld_encoding::DAG_CBOR;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Similar to the `CidHashMap` implementation, `FrozenCids` optimizes storage of
/// CIDs that would normally be stored as a vector of CIDs. The V1 DAG-CBOR
/// Blake2b-256 variant (which can be stored in 32 bytes vs 96 bytes for a `Cid`
/// type) is +99.99% of all CIDs, so very few CIDs need to be stored in the
/// `Heap(Box<Cid>)` variant of `SmallCid`. 
/// 
/// We use `Box<[...]>` to save memory, avoiding vector overallocation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FrozenCids(Box<[SmallCid]>);

impl Default for FrozenCids {
    fn default() -> Self {
        FrozenCids(Box::new([]))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum SmallCid {
    Heap(Box<Cid>),
    InlineDagCborV1([u8; 32]),
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
        if self.current_ix < self.buffer.0.len() {
            let cid = &self.buffer.0[self.current_ix];
            self.current_ix += 1;
            match cid {
                SmallCid::Heap(cid) => Some(*cid.clone()),
                SmallCid::Inline(bytes) => {
                    let mut cid = [0; BLAKE2B256_SIZE];
                    cid.copy_from_slice(bytes);
                    Some(Cid::new_v1(
                        DAG_CBOR,
                        multihash::Multihash::wrap(Blake2b256.into(), &cid)
                            .expect("failed to convert Blake2b digest to V1 DAG-CBOR Blake2b CID"),
                    ))
                }
            }
        } else {
            None
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
        FrozenCids::from(iter.into_iter().collect::<Vec<_>>())
    }
}

impl From<Vec<Cid>> for FrozenCids {
    fn from(cids: Vec<Cid>) -> Self {
        let mut small_cids = Vec::with_capacity(cids.len());
        for cid in cids {
            match cid.try_into() {
                Ok(CidVariant::V1DagCborBlake2b(bytes)) => small_cids.push(SmallCid::Inline(bytes)),
                _ => small_cids.push(SmallCid::Heap(Box::new(cid))),
            }
        }
        FrozenCids(small_cids.into_boxed_slice())
    }
}

impl From<FrozenCids> for Vec<Cid> {
    fn from(frozen_cids: FrozenCids) -> Self {
        Vec::<Cid>::from(&frozen_cids)
    }
}

impl From<&FrozenCids> for Vec<Cid> {
    fn from(frozen_cids: &FrozenCids) -> Self {
        let mut cids = Vec::with_capacity(frozen_cids.0.len());
        for cid in frozen_cids.into_iter() {
            match cid.try_into() {
                Ok(CidVariant::V1DagCborBlake2b(bytes)) => {
                    let mut digest = [0; BLAKE2B256_SIZE];
                    digest.copy_from_slice(&bytes);
                    cids.push(Cid::new_v1(
                        DAG_CBOR,
                        multihash::Multihash::wrap(Blake2b256.into(), &digest)
                            .expect("failed to convert Blake2b digest to V1 DAG-CBOR Blake2b CID"),
                    ))
                }
                _ => cids.push(cid),
            }
        }
        cids
    }
}

impl FrozenCids {
    pub fn push(&self, cid: Cid) -> Self {
        let mut cids = Vec::<Cid>::from(self);
        cids.push(cid);
        FrozenCids::from(cids)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, cid: Cid) -> bool {
        let cids = Vec::<Cid>::from(self);
        cids.contains(&cid)
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
