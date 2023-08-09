// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::utils::cid::CidVariant;
use cid::Cid;
use serde::{Deserialize, Serialize};

/// Similar to the `CidHashMap` implementation, `FrozenCids` optimizes storage of
/// CIDs that would normally be stored as a vector of CIDs. The `V1 DAG-CBOR Blake2b-256`
/// variant (which can be stored in 32 bytes vs 96 bytes for a `Cid`
/// type) is `+99.99%` of all CIDs, so very few CIDs need to be stored in the
/// `Generic(Box<Cid>)` variant of `CidVariant`.
///
/// We use `Box<[...]>` to save memory, avoiding vector overallocation.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrozenCids(Box<[CidVariant]>);

impl Default for FrozenCids {
    fn default() -> Self {
        FrozenCids(Box::new([]))
    }
}

pub struct Iter<'a> {
    cids: std::slice::Iter<'a, CidVariant>,
}

impl<'a> IntoIterator for &'a FrozenCids {
    type Item = Cid;
    type IntoIter = Iter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        Iter {
            cids: self.0.iter(),
        }
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = Cid;
    fn next(&mut self) -> Option<Self::Item> {
        match self.cids.next() {
            Some(cid) => match cid {
                CidVariant::V1DagCborBlake2b(bytes) => {
                    Some(Cid::from(CidVariant::V1DagCborBlake2b(*bytes)))
                }
                CidVariant::Generic(cid) => Some(*cid.clone()),
            },
            None => None,
        }
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
            match cid.into() {
                CidVariant::V1DagCborBlake2b(bytes) => {
                    small_cids.push(CidVariant::V1DagCborBlake2b(bytes))
                }
                _ => small_cids.push(CidVariant::Generic(Box::new(cid))),
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
            match cid.into() {
                CidVariant::V1DagCborBlake2b(bytes) => {
                    cids.push(Cid::from(CidVariant::V1DagCborBlake2b(bytes)))
                }
                _ => cids.push(cid),
            }
        }
        cids
    }
}

impl FrozenCids {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn contains(&self, cid: Cid) -> bool {
        let cid = CidVariant::from(cid);
        self.0.contains(&cid)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use cid::multihash::{self, MultihashDigest};
    use fvm_ipld_encoding::DAG_CBOR;
    use quickcheck::Arbitrary;
    use quickcheck_macros::quickcheck;

    impl Arbitrary for FrozenCids {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            // Although the vast majority of CIDs are V1DagCborBlake2b, we want to generate the variants of FrozenCids with equal probability.
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
