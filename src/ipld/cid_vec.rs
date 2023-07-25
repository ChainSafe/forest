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

impl From<CidVec> for &[Cid] {
    fn from(vec: CidVec) -> Self {
        //TODO: Add v1_dagcbor_blake2b_vec
        &vec.fallback_vec.as_slice()
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

    pub fn is_empty(&self) -> bool {
        self.v1_dagcbor_blake2b_vec.is_empty() && self.fallback_vec.is_empty()
    }

    pub fn to_vec(&self) -> Vec<Cid> {
        self.v1_dagcbor_blake2b_vec
            .iter()
            .map(|bytes| Cid::new_v1(DAG_CBOR, multihash::Code::Blake2b256.digest(bytes)))
            .chain(self.fallback_vec.iter().cloned())
            .collect()
    }

    pub fn contains(&self, cid: Cid) -> bool {
        match cid.try_into() {
            Ok(CidVariant::V1DagCborBlake2b(bytes)) => self.v1_dagcbor_blake2b_vec.contains(&bytes),
            Err(()) => self.fallback_vec.contains(&cid),
        }
    }

    pub fn cids(&self) -> &[Cid] {
        //TODO: Add v1_dagcbor_blake2b_vec
        &self.fallback_vec
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
    use super::*;
    use quickcheck::Arbitrary;

    impl Arbitrary for CidVec {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let arbitrary_vec: Vec<u32> = Vec::arbitrary(g);
            Self {
                v1_dagcbor_blake2b_vec: arbitrary_vec
                    .iter()
                    .map(|x| {
                        let mut bytes = [0u8; BLAKE2B256_SIZE];
                        bytes.copy_from_slice(&x.to_be_bytes());
                        bytes
                    })
                    .collect(),
                fallback_vec: Vec::arbitrary(g),
            }
        }
    }
}
