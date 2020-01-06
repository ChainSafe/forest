// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use super::errors::Error;
use blocks::{TipSetKeys, Tipset};
use cid::Cid;
use clock::ChainEpoch;
use std::collections::{hash_map::DefaultHasher, HashMap};
use std::hash::{Hash, Hasher};
/// TipSetMetadata is the type stored as the value in the TipIndex hashmap.  It contains
/// a tipset pointing to blocks, the root cid of the chain's state after
/// applying the messages in this tipset to it's parent state, and the cid of the receipts
/// for these messages.
#[derive(Clone, PartialEq, Debug)]
pub struct TipSetMetadata {
    // tipset_state_root is the root of aggregate state after applying tipset
    tipset_state_root: Cid,

    // tipset_receipts_root is receipts from all message contained within this tipset
    tipset_receipts_root: Cid,

    // tipset is the set of blocks that forms the tip set
    tipset: Tipset,
}

/// Trait to allow metadata to be indexed by multiple types of structs
pub trait Index {
    fn hash_key(&self) -> u64;
}
impl Index for ChainEpoch {
    fn hash_key(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash::<DefaultHasher>(&mut hasher);
        hasher.finish()
    }
}
impl Index for TipSetKeys {
    fn hash_key(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash::<DefaultHasher>(&mut hasher);
        hasher.finish()
    }
}

/// TipIndex tracks tipsets and their states by TipsetKeys and ChainEpoch
#[derive(Default)]
pub struct TipIndex {
    // metadata allows lookup of recorded Tipsets and their state roots
    // by TipsetKey and Epoch
    metadata: HashMap<u64, TipSetMetadata>,
}

impl TipIndex {
    /// constructor
    pub fn new() -> Self {
        Self {
            metadata: HashMap::new(),
        }
    }
    /// put adds an entry to TipIndex's hashmap
    /// After this call the input TipSetMetadata can be looked up by the TipsetKey of
    /// the tipset, or the tipset's epoch
    pub fn put(&mut self, meta: &TipSetMetadata) -> Result<(), Error> {
        if meta.tipset.is_empty() {
            return Err(Error::NoBlocks);
        }
        // retrieve parent cids to be used as hash map key
        let parent_key = meta.tipset.parents();
        // retrieve epoch to be used as hash map key
        let epoch_key = meta.tipset.tip_epoch();
        // insert value by parent_key into hash map
        self.metadata.insert(parent_key.hash_key(), meta.clone());
        // insert value by epoch_key into hash map
        self.metadata.insert(epoch_key.hash_key(), meta.clone());
        Ok(())
    }
    /// get returns the tipset given by hashed key
    fn get(&self, key: u64) -> Result<TipSetMetadata, Error> {
        self.metadata
            .get(&key)
            .cloned()
            .ok_or_else(|| Error::UndefinedKey("invalid metadata key".to_string()))
    }

    /// get_tipset returns a tipset
    pub fn get_tipset(&self, idx: &dyn Index) -> Result<Tipset, Error> {
        Ok(self.get(idx.hash_key()).map(|r| r.tipset)?)
    }
    /// get_tipset_state_root returns the tipset_state_root
    pub fn get_tipset_state_root(&self, idx: &dyn Index) -> Result<Cid, Error> {
        Ok(self.get(idx.hash_key()).map(|r| r.tipset_state_root)?)
    }
    /// get_tipset_receipts_root returns the tipset_receipts_root
    pub fn get_tipset_receipts_root(&self, idx: &dyn Index) -> Result<Cid, Error> {
        Ok(self.get(idx.hash_key()).map(|r| r.tipset_receipts_root)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use address::Address;
    use blocks::{BlockHeader, Ticket, Tipset, TxMeta};
    use cid::{Cid, Codec, Version};
    use clock::ChainEpoch;
    use crypto::VRFResult;

    const WEIGHT: u64 = 1;
    const CACHED_BYTES: u8 = 0;

    fn template_key(data: &[u8]) -> Cid {
        let h = multihash::encode(multihash::Hash::SHA2256, data).unwrap();
        Cid::new(Codec::DagProtobuf, Version::V1, &h)
    }

    // key_setup returns a vec of distinct CIDs
    pub fn key_setup() -> Vec<Cid> {
        return vec![template_key(b"test content")];
    }

    // template_header defines a block header used in testing
    fn template_header(ticket_p: Vec<u8>, cid: Cid, timestamp: u64) -> BlockHeader {
        let cids = key_setup();
        BlockHeader {
            parents: TipSetKeys {
                cids: vec![cids[0].clone()],
            },
            weight: WEIGHT,
            epoch: ChainEpoch::new(1),
            miner_address: Address::new_secp256k1(ticket_p.clone()).unwrap(),
            messages: TxMeta {
                bls_messages: cids[0].clone(),
                secp_messages: cids[0].clone(),
            },
            message_receipts: cids[0].clone(),
            state_root: cids[0].clone(),
            timestamp,
            ticket: Ticket {
                vrfproof: VRFResult::new(ticket_p),
            },
            bls_aggregate: vec![1, 2, 3],
            cached_cid: cid,
            cached_bytes: CACHED_BYTES,
        }
    }

    // header_setup returns a vec of block headers to be used for testing purposes
    pub fn header_setup() -> Vec<BlockHeader> {
        let data0: Vec<u8> = vec![1, 4, 3, 6, 7, 1, 2];
        let cids = key_setup();
        return vec![template_header(data0, cids[0].clone(), 1)];
    }

    pub fn setup() -> Tipset {
        let headers = header_setup();
        Tipset::new(headers).expect("tipset is invalid")
    }

    fn meta_setup() -> TipSetMetadata {
        let tip_set = setup();
        TipSetMetadata {
            tipset_state_root: tip_set.blocks()[0].state_root.clone(),
            tipset_receipts_root: tip_set.blocks()[0].message_receipts.clone(),
            tipset: tip_set,
        }
    }

    #[test]
    fn put_test() {
        let meta = meta_setup();
        let mut tip = TipIndex::new();
        assert!(tip.put(&meta).is_ok(), "error setting tip index hash map")
    }

    #[test]
    fn get_from_hashmap() {
        let meta = meta_setup();
        let mut tip = TipIndex::new();
        tip.put(&meta).unwrap();
        let mut hasher = DefaultHasher::new();
        meta.tipset.parents().hash::<DefaultHasher>(&mut hasher);
        let result = tip.get(hasher.finish()).unwrap();
        assert_eq!(result, meta);
    }

    #[test]
    fn get_tipset_by_parents() {
        let meta = meta_setup();
        let mut tip = TipIndex::new();
        tip.put(&meta).unwrap();
        let result = tip.get_tipset(&meta.tipset.parents()).unwrap();
        assert_eq!(result, meta.tipset);
    }

    #[test]
    fn get_state_root_by_parents() {
        let meta = meta_setup();
        let mut tip = TipIndex::new();
        tip.put(&meta).unwrap();
        let result = tip
            .get_tipset_receipts_root(&meta.tipset.parents())
            .unwrap();
        assert_eq!(result, meta.tipset_state_root);
    }

    #[test]
    fn get_receipts_root_by_parents() {
        let meta = meta_setup();
        let mut tip = TipIndex::new();
        tip.put(&meta).unwrap();
        let result = tip
            .get_tipset_receipts_root(&meta.tipset.parents())
            .unwrap();
        assert_eq!(result, meta.tipset_receipts_root);
    }

    #[test]
    fn get_tipset_by_epoch() {
        let meta = meta_setup();
        let mut tip = TipIndex::new();
        tip.put(&meta).unwrap();
        let result = tip.get_tipset(&meta.tipset.tip_epoch()).unwrap();
        assert_eq!(result, meta.tipset);
    }

    #[test]
    fn get_state_root_by_epoch() {
        let meta = meta_setup();
        let mut tip = TipIndex::new();
        tip.put(&meta).unwrap();
        let result = tip.get_tipset_state_root(&meta.tipset.tip_epoch()).unwrap();
        assert_eq!(result, meta.tipset_state_root);
    }

    #[test]
    fn get_receipts_root_by_epoch() {
        let meta = meta_setup();
        let mut tip = TipIndex::new();
        tip.put(&meta).unwrap();
        let result = tip
            .get_tipset_receipts_root(&meta.tipset.tip_epoch())
            .unwrap();
        assert_eq!(result, meta.tipset_receipts_root);
    }
}
