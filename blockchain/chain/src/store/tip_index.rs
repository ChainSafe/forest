#![allow(dead_code)]
use blocks::{TipSetKeys, Tipset};

use cid::Cid;
use std::collections::HashMap;

/// TipSetMetadata is the type stored at the leaves of the TipIndex.  It contains
/// a tipset pointing to blocks, the root cid of the chain's state after
/// applying the messages in this tipset to it's parent state, and the cid of the receipts
/// for these messages.
#[derive(Clone)]
pub struct TipSetMetadata {
    // tipset_state_root is the root of aggregate state after applying tipset
    tipset_state_root: Cid,

    // tipset is the set of blocks that forms the tip set
    tipset: Tipset,

    // tipset_receipts receipts from all message contained within this tipset
    tipset_receipts: Cid,
}

/// TipIndex tracks tipsets and their states by tipset block ids and parent
/// block ids.
pub struct TipIndex {
    // metadata_by_tipset_key allows lookup of recorded Tipsets and their state roots by TipsetKey as bytes.
    metadata_by_tipset_key: HashMap<TipSetKeys, TipSetMetadata>,
}

impl TipIndex {
    /// constructor 
    pub fn new() {}
    /// put adds an entry to TipIndex's internal indexes.
    /// After this call the input TipSetMetadata can be looked up by the TipsetKey of
    /// the tipset, or the tipset's parent
    pub fn put() {}
    /// get returns the tipset given by the input ID and its state.
    pub fn get(&self, key: TipSetKeys) -> TipSetMetadata {
        self.metadata_by_tipset_key[&key].clone()
    }
    pub fn has() {}
    /// get_tipset returns the tipset from func (ti *TipIndex) Get(tsKey string)
    pub fn get_tipset(&self, key: TipSetKeys) -> Tipset {
        let t: TipSetMetadata = self.get(key);
        t.tipset
    }
    /// get_tipset_state_root returns the tipsetStateRoot from func (ti *TipIndex) Get(tsKey string).
    pub fn get_tipset_state_root(&self, key: TipSetKeys) -> Cid {
        let t: TipSetMetadata = self.get(key);
        t.tipset_state_root
    }
    /// get_tipset_receipts_root returns the tipsetReceipts from func (ti *TipIndex) Get(tsKey string).
    pub fn get_tipset_receipts_root(&self, key: TipSetKeys) -> Cid {
        let t: TipSetMetadata = self.get(key);
        t.tipset_receipts
    }

    pub fn get_by_parents_and_epoch() {}
    pub fn has_by_parents_and_epoch() {}
}
