// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::ipld::CidHashMap;
use cid::Cid;

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
