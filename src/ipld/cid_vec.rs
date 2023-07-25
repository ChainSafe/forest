// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::utils::cid::{CidVariant, BLAKE2B256_SIZE};
use cid::{
    multihash::{self, MultihashDigest},
    Cid,
};
use fvm_ipld_encoding::DAG_CBOR;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct CidVec {
    pub v1_dagcbor_blake2b_vec: Vec<[u8; BLAKE2B256_SIZE]>,
    pub fallback_vec: Vec<Cid>,
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
        vec.into_iter().collect()
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
        Self {
            v1_dagcbor_blake2b_vec: Vec::new(),
            fallback_vec: Vec::new(),
        }
    }

    pub fn new_from_cid(cid: Cid) -> Self {
        let mut vec = Self::new();
        vec.push(cid);
        vec
    }

    pub fn push(&mut self, cid: Cid) {
        match cid.try_into() {
            Ok(CidVariant::V1DagCborBlake2b(bytes)) => self.v1_dagcbor_blake2b_vec.push(bytes),
            Err(()) => self.fallback_vec.push(cid),
        }
    }

    pub fn cids(&self) -> Vec<Cid> {
        self.v1_dagcbor_blake2b_vec
            .iter()
            .map(|bytes| Cid::new_v1(DAG_CBOR, multihash::Code::Blake2b256.digest(bytes)))
            .into_iter()
            .chain(self.fallback_vec.clone())
            .collect()
    }
}

impl Iterator for CidVec {
    type Item = Cid;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(bytes) = self.v1_dagcbor_blake2b_vec.pop() {
            Some(Cid::new_v1(
                DAG_CBOR,
                multihash::Code::Blake2b256.digest(&bytes),
            ))
        } else {
            self.fallback_vec.pop()
        }
    }
}

#[cfg(test)]
mod test {
    use crate::utils::encoding::blake2b_256;

    use super::*;
    use quickcheck::Arbitrary;
    use quickcheck_macros::quickcheck;

    impl Arbitrary for CidVec {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let arbitrary_vec: Vec<u64> = Vec::arbitrary(g);
            Self {
                v1_dagcbor_blake2b_vec: arbitrary_vec
                    .iter()
                    .map(|i| blake2b_256(&i.to_be_bytes()))
                    .collect(),
                fallback_vec: Vec::arbitrary(g),
            }
        }
    }

    #[quickcheck]
    fn cidvec_to_vec_of_cids_to_cidvec(cidvec: CidVec) {
        assert_eq!(cidvec, CidVec::from(Vec::<Cid>::from(cidvec.clone())));
    }
}
