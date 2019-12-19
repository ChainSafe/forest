use super::errors::Error;
use blocks::{TipSetKeys, Tipset};
use cid::Cid;
use clock::ChainEpoch;
use std::collections::HashMap;

/// TipSetMetadata is the type stored as the value in the TipIndex hashmap.  It contains
/// a tipset pointing to blocks, the root cid of the chain's state after
/// applying the messages in this tipset to it's parent state, and the cid of the receipts
/// for these messages.
#[derive(Clone, PartialEq, Debug)]
pub struct TipSetMetadata {
    // tipset_state_root is the root of aggregate state after applying tipset
    tipset_state_root: Cid,

    // tipset is the set of blocks that forms the tip set
    tipset: Tipset,

    // tipset_receipts_root is receipts from all message contained within this tipset
    tipset_receipts_root: Cid,
}

/// TipIndex tracks tipsets and their states by TipsetKeys and ChainEpoch
#[derive(Default)]
pub struct TipIndex {
    // metadata_by_tipset_key allows lookup of recorded Tipsets and their state roots by TipsetKey
    metadata_by_parent_key: HashMap<TipSetKeys, TipSetMetadata>,
    // metadata_by_tipset_key allows lookup of recorded Tipsets and their state roots by Epoch
    metadata_by_epoch: HashMap<ChainEpoch, TipSetMetadata>,
}

impl TipIndex {
    /// constructor
    pub fn new() -> Self {
        Self {
            metadata_by_parent_key: HashMap::new(),
            metadata_by_epoch: HashMap::new(),
        }
    }
    /// put adds an entry to TipIndex's hashmap
    /// After this call the input TipSetMetadata can be looked up by the TipsetKey of
    /// the tipset, or the tipset's epoch
    pub fn put(&mut self, meta: TipSetMetadata) -> Result<bool, Error> {
        if meta.tipset.is_empty() {
            return Err(Error::NoBlocks);
        }
        // retrieve parent cids to be used as hash map key
        let parent_key = meta.tipset.parents();
        // retrieve epoch to be used as hash map key
        let epoch_key = meta.tipset.tip_epoch();
        // insert value by parent_key into hash map
        self.metadata_by_parent_key
            .insert(parent_key.clone(), meta.clone());
        // insert value by epoch_key into hash map
        self.metadata_by_epoch.insert(epoch_key, meta.clone());
        Ok(true)
    }
    /// get_by_parent_key returns the tipset given by TipSetKey(e.g. parents())
    pub fn get_by_parent_key(&self, key: TipSetKeys) -> Result<TipSetMetadata, Error> {
        match self.metadata_by_parent_key.get(&key) {
            Some(val) => Ok(val.clone()),
            _ => Err(Error::UndefinedKey("invalid TipSetKey key".to_string())),
        }
    }
    /// get_by_epoch_key returns the tipset given by ChainEpoch(e.g. tip_epoch())
    pub fn get_by_epoch_key(&self, key: ChainEpoch) -> Result<TipSetMetadata, Error> {
        match self.metadata_by_epoch.get(&key) {
            Some(val) => Ok(val.clone()),
            _ => Err(Error::UndefinedKey("invalid chain epoch key".to_string())),
        }
    }

    /// get_tipset_by_parents returns a tipset by TipSetKeys
    pub fn get_tipset_by_parents(&self, key: TipSetKeys) -> Result<Tipset, Error> {
        let result = self.get_by_parent_key(key)?;
        Ok(result.tipset)
    }

    /// get_tipset_by_epoch returns a tipset by ChainEpoch
    pub fn get_tipset_by_epoch(&self, key: ChainEpoch) -> Result<Tipset, Error> {
        let result = self.get_by_epoch_key(key)?;
        Ok(result.tipset)
    }

    /// get_tipset_state_root_by_parents returns the tipset_state_root
    pub fn get_tipset_state_root_by_parents(&self, key: TipSetKeys) -> Result<Cid, Error> {
        let result = self.get_by_parent_key(key)?;
        Ok(result.tipset_state_root)
    }

    /// get_tipset_state_root_by_epoch returns the tipset_state_root
    pub fn get_tipset_state_root_by_epoch(&self, key: ChainEpoch) -> Result<Cid, Error> {
        let result = self.get_by_epoch_key(key)?;
        Ok(result.tipset_state_root)
    }

    /// get_tipset_receipts_root_by_parents returns the tipset_receipts_root
    pub fn get_tipset_receipts_root_by_parents(&self, key: TipSetKeys) -> Result<Cid, Error> {
        let result = self.get_by_parent_key(key)?;
        Ok(result.tipset_receipts_root)
    }

    /// get_tipset_receipts_root_by_epoch returns the tipset_receipts_root
    pub fn get_tipset_receipts_root_by_epoch(&self, key: ChainEpoch) -> Result<Cid, Error> {
        let result = self.get_by_epoch_key(key)?;
        Ok(result.tipset_state_root)
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
        return vec![template_header(data0.clone(), cids[0].clone(), 1)];
    }

    pub fn setup() -> Tipset {
        let headers = header_setup();
        Tipset::new(headers.clone()).expect("tipset is invalid")
    }

    fn meta_setup() -> TipSetMetadata {
        let tip_set = setup();
        TipSetMetadata {
            tipset_state_root: tip_set.blocks[0].state_root.clone(),
            tipset: tip_set.clone(),
            tipset_receipts_root: tip_set.blocks[0].message_receipts.clone(),
        }
    }

    #[test]
    fn put_test() {
        let meta = meta_setup();
        let mut tip = TipIndex::new();
        assert!(
            TipIndex::put(&mut tip, meta.clone()).is_ok(),
            "error setting tip index hash map"
        )
    }

    #[test]
    fn get_by_parent_key_test() {
        let meta = meta_setup();
        let mut tip = TipIndex::new();
        TipIndex::put(&mut tip, meta.clone()).unwrap();
        let result = TipIndex::get_by_parent_key(&tip, meta.tipset.parents()).unwrap();
        assert_eq!(result, meta.clone());
    }

    #[test]
    fn get_by_epoch_key_test() {
        let meta = meta_setup();
        let mut tip = TipIndex::new();
        TipIndex::put(&mut tip, meta.clone()).unwrap();
        let result = TipIndex::get_by_epoch_key(&tip, meta.tipset.tip_epoch()).unwrap();
        assert_eq!(result, meta.clone());
    }

    #[test]
    fn get_tipset_by_parents_test() {
        let meta = meta_setup();
        let mut tip = TipIndex::new();
        TipIndex::put(&mut tip, meta.clone()).unwrap();
        let result = TipIndex::get_tipset_by_parents(&tip, meta.tipset.parents()).unwrap();
        assert_eq!(result, meta.tipset.clone());
    }

    #[test]
    fn get_state_root_by_parents_test() {
        let meta = meta_setup();
        let mut tip = TipIndex::new();
        TipIndex::put(&mut tip, meta.clone()).unwrap();
        let result =
            TipIndex::get_tipset_state_root_by_parents(&tip, meta.tipset.parents()).unwrap();
        assert_eq!(result, meta.tipset_state_root.clone());
    }

    #[test]
    fn get_receipts_root_by_parents() {
        let meta = meta_setup();
        let mut tip = TipIndex::new();
        TipIndex::put(&mut tip, meta.clone()).unwrap();
        let result =
            TipIndex::get_tipset_receipts_root_by_parents(&tip, meta.tipset.parents()).unwrap();
        assert_eq!(result, meta.tipset_receipts_root.clone());
    }

    #[test]
    fn get_tipset_by_epoch_test() {
        let meta = meta_setup();
        let mut tip = TipIndex::new();
        TipIndex::put(&mut tip, meta.clone()).unwrap();
        let result = TipIndex::get_tipset_by_epoch(&tip, meta.tipset.tip_epoch()).unwrap();
        assert_eq!(result, meta.tipset.clone());
    }

    #[test]
    fn get_state_root_by_epoch_test() {
        let meta = meta_setup();
        let mut tip = TipIndex::new();
        TipIndex::put(&mut tip, meta.clone()).unwrap();
        let result =
            TipIndex::get_tipset_state_root_by_epoch(&tip, meta.tipset.tip_epoch()).unwrap();
        assert_eq!(result, meta.tipset_state_root.clone());
    }

    #[test]
    fn get_receipts_root_by_epoch() {
        let meta = meta_setup();
        let mut tip = TipIndex::new();
        TipIndex::put(&mut tip, meta.clone()).unwrap();
        let result =
            TipIndex::get_tipset_receipts_root_by_epoch(&tip, meta.tipset.tip_epoch()).unwrap();
        assert_eq!(result, meta.tipset_receipts_root.clone());
    }
}
