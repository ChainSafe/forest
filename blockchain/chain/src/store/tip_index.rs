#![allow(dead_code)]
use crate::blocks::TipSet;

use cid::Cid;
use std::collections::HashMap;

// TipSetMetadata is the type stored at the leaves of the TipIndex.  It contains
// a tipset pointing to blocks, the root cid of the chain's state after
// applying the messages in this tipset to it's parent state, and the cid of the receipts
// for these messages.
struct TipSetMetadata {
    // tipset_state_root is the root of aggregate state after applying tipset
    tipset_state_root: Cid,

    // tipset is the set of blocks that forms the tip set
    tipset: TipSet,

    // tipset_receipts receipts from all message contained within this tipset
    tipset_receipts: Cid,
}

// TipIndex tracks tipsets and their states by tipset block ids and parent
// block ids.
pub struct TipIndex<'a> {
    // metadata_by_tipset_id allows lookup of recorded TipSetAndStates by TipSet ID.
    metadata_by_tipset_id: HashMap<&'a str, TipSetMetadata>,
    // tsasByEpoch allows lookup of all TipSetAndStates with the same parent IDs.
    tsas_by_parents: HashMap<TipSet::parents, metadata_by_tipset_id>,
}

fn make_key(tipset_key: String, epoch: ChainEpoch) -> String {

}

// tipsetKey :  TipSetMetadata
// epoch-tipsetKey : above value

impl TipIndex {
    pub fn put() {}
    pub fn get() {}
    pub fn has() {}
    pub fn get_tipset() {}
    pub fn get_tipset_stateroot() {}
    pub fn get_tipset_receiptsroot() {}
    pub fn get_by_parents_and_epoch() {}
    pub fn has_by_parents_and_epoch() {}
}
