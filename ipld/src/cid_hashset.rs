// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::{HashSet, HashSetExt, RandomState};
use cid::Cid;

pub struct CidHashSet {
    set: HashSet<u64>,
    hasher: RandomState,
}

impl CidHashSet {
    pub fn insert(&mut self, cid: &Cid) -> bool {
        let hash = self.hasher.hash_one(cid.to_bytes());
        self.set.insert(hash)
    }
}

impl Default for CidHashSet {
    fn default() -> Self {
        Self {
            set: HashSet::new(),
            hasher: RandomState::new(),
        }
    }
}
