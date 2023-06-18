// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashSet;
use cid::{Cid, Version};
use fvm_ipld_encoding::DAG_CBOR;
use std::collections::BTreeSet;

// Nearly all Filecoin CIDs are V1, DagCbor encoded, and hashed with Blake2b265. If other types of
// CID become popular, they can be added to the CidVariant structure.
enum CidVariant {
    V1DagCborBlake2b([u8; 32]),
}

impl TryFrom<Cid> for CidVariant {
    type Error = ();
    fn try_from(cid: Cid) -> Result<Self, Self::Error> {
        if cid.version() == Version::V1 {
            if cid.codec() == DAG_CBOR {
                if let Ok(small_hash) = cid.hash().resize() {
                    let (code, bytes, size) = small_hash.into_inner();
                    if code == BLAKE2B256 && size == 32 {
                        return Ok(CidVariant::V1DagCborBlake2b(bytes));
                    }
                }
            }
        }
        Err(())
    }
}

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
    v1_dagcbor_blake2b: HashSet<[u8; 32]>,
    fallback: HashSet<Cid>,
}

const BLAKE2B256: u64 = 0xb220;

impl CidHashSet {
    pub fn insert(&mut self, &cid: &Cid) -> bool {
        match cid.try_into() {
            Ok(CidVariant::V1DagCborBlake2b(bytes)) => self.v1_dagcbor_blake2b.insert(bytes),
            Err(()) => self.fallback.insert(cid),
        }
    }

    pub fn len(&self) -> usize {
        self.v1_dagcbor_blake2b.len() + self.fallback.len()
    }

    pub fn is_empty(&self) -> bool {
        self.v1_dagcbor_blake2b.is_empty()
            && self.fallback.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use cid::{multihash::MultihashDigest, Cid};
    use std::mem::size_of;

    // If this stops being true, please update the documentation above.
    #[test]
    fn cid_size_assumption() {
        assert_eq!(size_of::<Cid>(), 96);
    }

    #[test]
    fn blake_code_assumption() {
        assert_eq!(
            cid::multihash::Code::Blake2b256.digest(&[]).code(),
            super::BLAKE2B256
        );
    }
}
