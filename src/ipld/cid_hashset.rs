// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::utils::cid::{CidVariant, BLAKE2B256_SIZE};
use ahash::HashSet;
use cid::{Cid, Version};
use fvm_ipld_encoding::DAG_CBOR;
use std::collections::BTreeSet;

// The size of a CID is 96 bytes. A CID contains:
//   - a version
//   - a codec
//   - a hash code
//   - a length
//   - 64 bytes pre-allocated buffer
// Each non-buffer field takes 8 bytes with padding. So, 4*8 = 32 bytes, 32 + 64 = 96 bytes.
//
// However, we know that nearly all Filecoin CIDs have version=V1, codec=DAG_CBOR, code=Blake2b and
// length=32. Taking advantage of this knowledge, we can store the vast majority of CIDs (+99.99%)
// in one third of the usual space (32 bytes vs 96 bytes).
#[derive(Default)]
pub struct CidHashSet {
    v1_dagcbor_blake2b: HashSet<[u8; BLAKE2B256_SIZE]>,
    fallback: HashSet<Cid>,
}

impl CidHashSet {
    pub fn insert(&mut self, cid: Cid) -> bool {
        match cid.try_into() {
            Ok(CidVariant::V1DagCborBlake2b(bytes)) => self.v1_dagcbor_blake2b.insert(bytes),
            Err(()) => self.fallback.insert(cid),
        }
    }

    pub fn len(&self) -> usize {
        self.v1_dagcbor_blake2b.len() + self.fallback.len()
    }

    pub fn is_empty(&self) -> bool {
        self.v1_dagcbor_blake2b.is_empty() && self.fallback.is_empty()
    }
}

#[cfg(test)]
mod tests {}
