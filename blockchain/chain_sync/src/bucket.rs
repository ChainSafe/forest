// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use blocks::Tipset;
use std::sync::Arc;

/// SyncBucket defines a bucket of tipsets to sync
#[derive(Clone, Default, PartialEq, PartialOrd, Ord, Eq)]
pub struct SyncBucket {
    tips: Vec<Arc<Tipset>>,
}

impl SyncBucket {
    /// Constructor for tipset bucket
    fn new(tips: Vec<Arc<Tipset>>) -> SyncBucket {
        Self { tips }
    }
    /// Returns the tipset with the max weight
    pub fn heaviest_tipset(&self) -> Option<Arc<Tipset>> {
        if self.tips.is_empty() {
            return None;
        }

        // return max value pointer
        self.tips.iter().max_by_key(|a| a.weight()).cloned()
    }
    /// Returns true if tipset is from same chain
    pub fn same_chain_as(&mut self, ts: &Tipset) -> bool {
        for t in self.tips.iter_mut() {
            // TODO Confirm that comparing keys will be sufficient on full tipset impl
            if ts.key() == t.key() || ts.key() == t.parents() || ts.parents() == t.key() {
                return true;
            }
        }

        false
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
        for b in self.buckets.iter_mut() {
            if b.same_chain_as(&tipset) {
                b.add(tipset);
                return;
            }
        }
        self.buckets.push(SyncBucket::new(vec![tipset]))
    }
    /// Removes the SyncBucket with heaviest weighted Tipset from SyncBucketSet
    pub(crate) fn pop(&mut self) -> Option<SyncBucket> {
        if let Some((i, _)) = self
            .buckets()
            .iter()
            .enumerate()
            .max_by_key(|(_, b)| b.heaviest_tipset())
        {
            let ts = self.buckets.remove(i);
            Some(ts)
        } else {
            None
        }
    }
    /// Returns heaviest tipset from bucket set
    pub(crate) fn heaviest(&self) -> Option<Arc<Tipset>> {
        // Transform max values from each bucket into a Vec
        let vals: Vec<Arc<Tipset>> = self
            .buckets
            .iter()
            .filter_map(|b| b.heaviest_tipset())
            .collect();

        // Return the heaviest tipset bucket
        vals.iter().max_by_key(|b| b.weight()).cloned()
    }
    /// Returns a vector of SyncBuckets
    pub(crate) fn buckets(&self) -> &[SyncBucket] {
        &self.buckets
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blocks::BlockHeader;
    use cid::{multihash::Blake2b256, Cid};
    use num_bigint::BigUint;

    fn create_header(weight: u64, parent_bz: &[u8], cached_bytes: &[u8]) -> BlockHeader {
        let header = BlockHeader::builder()
            .weight(BigUint::from(weight))
            .cached_bytes(cached_bytes.to_vec())
            .cached_cid(Cid::new_from_cbor(parent_bz, Blake2b256).unwrap())
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
            &BigUint::from(3u8)
        );
        assert_eq!(bucket.tips.len(), 2);

        // assert bucket with just one tipset still resolves
        let bucket = SyncBucket::new(vec![l_tip]);
        assert_eq!(
            bucket.heaviest_tipset().unwrap().weight(),
            &BigUint::from(1u8)
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
