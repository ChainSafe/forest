// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use blocks::Tipset;
use num_bigint::BigInt;
use std::sync::Arc;

/// SyncBucket defines a bucket of tipsets to sync
#[derive(Clone, Default, PartialEq, Eq)]
pub struct SyncBucket {
    tips: Vec<Arc<Tipset>>,
}

impl SyncBucket {
    /// Constructor for tipset bucket
    fn new(tips: Vec<Arc<Tipset>>) -> SyncBucket {
        Self { tips }
    }
    /// Returns the weight of the heaviest tipset
    fn max_weight(&self) -> Option<&BigInt> {
        self.tips.iter().map(|ts| ts.weight()).max()
    }
    /// Returns the tipset with the max weight
    pub fn heaviest_tipset(&self) -> Option<Arc<Tipset>> {
        self.tips.iter().max_by_key(|a| a.weight()).cloned()
    }
    /// Returns true if tipset is from same chain
    pub fn is_same_chain_as(&self, ts: &Tipset) -> bool {
        // TODO Confirm that comparing keys will be sufficient on full tipset impl
        self.tips
            .iter()
            .any(|t| ts.key() == t.key() || ts.key() == t.parents() || ts.parents() == t.key())
    }
    /// Adds tipset to vector to be included in the bucket
    pub fn add(&mut self, ts: Arc<Tipset>) {
        if !self.tips.iter().any(|t| *t == ts) {
            self.tips.push(ts);
        }
    }
    /// Returns true if SyncBucket is empty
    pub fn is_empty(&self) -> bool {
        self.tips.is_empty()
    }
}

/// Set of tipset buckets
#[derive(Default, Clone)]
pub(crate) struct SyncBucketSet {
    buckets: Vec<SyncBucket>,
}

impl SyncBucketSet {
    /// Inserts a tipset into a bucket
    pub(crate) fn insert(&mut self, tipset: Arc<Tipset>) {
        if let Some(b) = self
            .buckets
            .iter_mut()
            .find(|b| b.is_same_chain_as(&tipset))
        {
            b.add(tipset);
        } else {
            self.buckets.push(SyncBucket::new(vec![tipset]))
        }
    }
    /// Removes the SyncBucket with heaviest weighted Tipset from SyncBucketSet
    pub(crate) fn pop(&mut self) -> Option<SyncBucket> {
        let (i, _) = self
            .buckets()
            .iter()
            .enumerate()
            .map(|(i, b)| (i, b.max_weight()))
            .max_by(|(_, w1), (_, w2)| w1.cmp(w2))?;
        // we can't use `max_by_key` here because the weight is a reference,
        // see https://github.com/rust-lang/rust/issues/34162

        Some(self.buckets.swap_remove(i))
    }
    /// Returns heaviest tipset from bucket set
    pub(crate) fn heaviest(&self) -> Option<Arc<Tipset>> {
        self.buckets
            .iter()
            .filter_map(SyncBucket::heaviest_tipset)
            .max_by(|ts1, ts2| ts1.weight().cmp(ts2.weight()))
    }
    /// Returns a vector of SyncBuckets
    pub(crate) fn buckets(&self) -> &[SyncBucket] {
        &self.buckets
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use address::Address;
    use blocks::BlockHeader;
    use cid::{multihash::Blake2b256, Cid};
    use num_bigint::BigInt;

    fn create_header(weight: u64, parent_bz: &[u8], cached_bytes: &[u8]) -> BlockHeader {
        let header = BlockHeader::builder()
            .weight(BigInt::from(weight))
            .cached_bytes(cached_bytes.to_vec())
            .cached_cid(Cid::new_from_cbor(parent_bz, Blake2b256))
            .miner_address(Address::new_id(0))
            .build()
            .unwrap();
        header
    }

    #[test]
    fn base_bucket_constructor() {
        SyncBucket::new(Vec::new());
    }

    #[test]
    fn heaviest_tipset() {
        let l_tip = Arc::new(Tipset::new(vec![create_header(1, b"", b"")]).unwrap());
        let h_tip = Arc::new(Tipset::new(vec![create_header(3, b"", b"")]).unwrap());

        // Test the comparison of tipsets
        let bucket = SyncBucket::new(vec![l_tip.clone(), h_tip]);
        assert_eq!(
            bucket.heaviest_tipset().unwrap().weight(),
            &BigInt::from(3u8)
        );
        assert_eq!(bucket.tips.len(), 2);

        // assert bucket with just one tipset still resolves
        let bucket = SyncBucket::new(vec![l_tip]);
        assert_eq!(
            bucket.heaviest_tipset().unwrap().weight(),
            &BigInt::from(1u8)
        );
    }

    #[test]
    fn sync_bucket_inserts() {
        let mut set = SyncBucketSet::default();
        let tipset1 = Arc::new(Tipset::new(vec![create_header(1, b"1", b"1")]).unwrap());
        set.insert(tipset1.clone());
        assert_eq!(set.buckets.len(), 1);
        assert_eq!(set.buckets[0].tips.len(), 1);

        // Assert a tipset on non relating chain is put in another bucket
        let tipset2 = Arc::new(Tipset::new(vec![create_header(2, b"2", b"2")]).unwrap());
        set.insert(tipset2);
        assert_eq!(
            set.buckets.len(),
            2,
            "Inserting seperate tipset should create new bucket"
        );
        assert_eq!(set.buckets[1].tips.len(), 1);

        // Assert a tipset connected to the first
        let tipset3 = Arc::new(Tipset::new(vec![create_header(3, b"1", b"1")]).unwrap());
        assert_eq!(tipset1.key(), tipset3.key());
        set.insert(tipset3);
        assert_eq!(
            set.buckets.len(),
            2,
            "Inserting into first chain should not create 3rd bucket"
        );
        assert_eq!(
            set.buckets[0].tips.len(),
            2,
            "Should be 2 tipsets in bucket 0"
        );

        // Assert that tipsets that are already added are not added twice
        set.insert(tipset1);
        assert_eq!(set.buckets.len(), 2);
        assert_eq!(set.buckets[0].tips.len(), 2);
    }
}
