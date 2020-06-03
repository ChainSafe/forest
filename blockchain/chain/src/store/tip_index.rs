// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::errors::Error;
use blocks::{Tipset, TipsetKeys};
use cid::Cid;
use clock::ChainEpoch;
use std::collections::hash_map::{DefaultHasher, HashMap};
use std::hash::{Hash, Hasher};
/// TipsetMetadata is the type stored as the value in the TipIndex hashmap.  It contains
/// a tipset pointing to blocks, the root cid of the chain's state after
/// applying the messages in this tipset to it's parent state, and the cid of the receipts
/// for these messages.
#[derive(Clone, PartialEq, Debug)]
pub struct TipsetMetadata {
    /// Root of aggregate state after applying tipset
    pub tipset_state_root: Cid,

    /// Receipts from all message contained within this tipset
    pub tipset_receipts_root: Cid,

    /// The actual Tipset
    pub tipset: Tipset,
}

/// Trait to allow metadata to be indexed by multiple types of structs
pub trait Index: Hash {
    fn hash_key(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash::<DefaultHasher>(&mut hasher);
        hasher.finish()
    }
}
impl Index for ChainEpoch {}
impl Index for TipsetKeys {}

/// Tracks tipsets and their states by TipsetKeys and ChainEpoch.
#[derive(Default)]
pub struct TipIndex {
    // metadata allows lookup of recorded Tipsets and their state roots
    // by TipsetKey and Epoch
    metadata: HashMap<u64, TipsetMetadata>,
}

impl TipIndex {
    /// Creates new TipIndex with empty metadata
    pub fn new() -> Self {
        Self {
            metadata: HashMap::new(),
        }
    }
    /// Adds an entry to TipIndex's metadata
    /// After this call the input TipsetMetadata can be looked up by the TipsetKey of
    /// the tipset, or the tipset's epoch
    pub fn put(&mut self, meta: &TipsetMetadata) {
        // retrieve parent cids to be used as hash map key
        let parent_key = meta.tipset.parents();
        // retrieve epoch to be used as hash map key
        let epoch_key = meta.tipset.epoch();
        // insert value by parent_key into hash map
        self.metadata.insert(parent_key.hash_key(), meta.clone());
        // insert value by epoch_key into hash map
        self.metadata.insert(epoch_key.hash_key(), meta.clone());
    }
    /// Returns the tipset given by hashed key
    fn get(&self, key: u64) -> Result<TipsetMetadata, Error> {
        self.metadata
            .get(&key)
            .cloned()
            .ok_or_else(|| Error::UndefinedKey("invalid metadata key".to_string()))
    }

    /// Returns the tipset corresponding to the hashed index
    pub fn get_tipset<I: Index>(&self, idx: &I) -> Result<Tipset, Error> {
        Ok(self.get(idx.hash_key()).map(|r| r.tipset)?)
    }
    /// Returns the state root for the tipset corresponding to the index
    pub fn get_tipset_state_root<I: Index>(&self, idx: &I) -> Result<Cid, Error> {
        Ok(self.get(idx.hash_key()).map(|r| r.tipset_state_root)?)
    }
    /// Returns the receipt root for the tipset corresponding to the index
    pub fn get_tipset_receipts_root<I: Index>(&self, idx: &I) -> Result<Cid, Error> {
        Ok(self.get(idx.hash_key()).map(|r| r.tipset_receipts_root)?)
    }
}
