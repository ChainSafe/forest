// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::{HashSet, HashSetExt};
use cid::Cid;

pub struct CidHashSet {
    mem: HashSet<Vec<u8>>,
}

impl CidHashSet {
    pub fn new() -> anyhow::Result<Self> {
        let mem = HashSet::new();
        Ok(CidHashSet { mem })
    }

    pub fn insert(&mut self, cid: &Cid) -> bool {
        let hash = cid.hash().digest();
        self.mem.insert(hash.to_vec())
    }
}
