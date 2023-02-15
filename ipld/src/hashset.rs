// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::{HashSet, HashSetExt, RandomState};
use cid::Cid;

pub struct CidHashSet {
    mem: HashSet<u64>,
    hasher: RandomState,
}

impl CidHashSet {
    pub fn new() -> anyhow::Result<Self> {
        let mem = HashSet::new();
        Ok(CidHashSet {
            mem,
            hasher: RandomState::new(),
        })
    }

    pub fn insert(&mut self, cid: &Cid) -> bool {
        // let hash = cid.hash().digest();
        // // self.mem.insert(hash[..hash.len().min(16)].to_vec())
        // self.mem.insert(hash.to_vec())
        let hash = self.hasher.hash_one(cid.to_bytes());
        self.mem.insert(hash)
    }
}
