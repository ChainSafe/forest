// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::ipld::CidHashMap;
use cid::Cid;

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
pub struct CidHashSet(CidHashMap<()>);

impl CidHashSet {
    /// Adds a value to the set if not already present and returns whether the value was newly inserted.
    pub fn insert(&mut self, cid: Cid) -> bool {
        self.0.insert(cid, ()).is_none()
    }

    /// Returns the number of items in the set.
    pub fn len(&self) -> usize {
        self.0.len()
    }
}
