// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use blocks::Tipset;
use num_bigint::BigInt;
use std::sync::Arc;

/// SyncBucket defines a bucket of [Tipsets] to sync.
/// All tipsets in a bucket are connected on the same chain.
#[derive(Clone, Default, PartialEq)]
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
        // TODO can maybe short circuit when keys equivalent, instead of checking on add
        #[allow(clippy::all)]
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
}

/// Set of tipset buckets. This keeps track of all individual groupings of [Tipset]s.
#[derive(Default, Clone)]
pub(crate) struct SyncBucketSet {
    buckets: Vec<SyncBucket>,
}

impl SyncBucketSet {
    /// Inserts a tipset into a bucket. This will either add to an existing bucket, if [Tipset]
    /// is connected, creates new [SyncBucket] if not.
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
    /// Removes the [SyncBucket] with heaviest weighted Tipset from [SyncBucketSet]
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

    /// Returns true if tipset is related to any tipset in the bucket set.
    pub(crate) fn related_to_any(&self, ts: &Tipset) -> bool {
        for b in self.buckets.iter() {
            if b.is_same_chain_as(ts) {
                return true;
            }
        }
        false
    }

    /// Returns a reference to the [SyncBucket]s.
    pub(crate) fn buckets(&self) -> &[SyncBucket] {
        &self.buckets
    }

    /// Heaviest tipset among all the buckets.
    pub(crate) fn heaviest(&self) -> Option<Arc<Tipset>> {
        self.buckets()
            .iter()
            .filter_map(|b| b.heaviest_tipset())
            .max_by_key(|ts| ts.weight().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use address::Address;
    use blocks::BlockHeader;
    use num_bigint::BigInt;

    fn create_header(weight: u64) -> BlockHeader {
        let header = BlockHeader::builder()
            .weight(BigInt::from(weight))
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
        let l_tip = Arc::new(Tipset::new(vec![create_header(1)]).unwrap());
        let h_tip = Arc::new(Tipset::new(vec![create_header(3)]).unwrap());

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
        let tipset1 = Arc::new(Tipset::new(vec![create_header(1)]).unwrap());
        set.insert(tipset1.clone());
        assert_eq!(set.buckets.len(), 1);
        assert_eq!(set.buckets[0].tips.len(), 1);

        // Assert a tipset on non relating chain is put in another bucket
        let tipset2 = Arc::new(Tipset::new(vec![create_header(2)]).unwrap());
        set.insert(tipset2);
        assert_eq!(
            set.buckets.len(),
            2,
            "Inserting separate tipset should create new bucket"
        );
        assert_eq!(set.buckets[1].tips.len(), 1);

        // Assert a tipset connected to the first
        let tipset3 = Arc::new(
            Tipset::new(vec![BlockHeader::builder()
                .weight(3.into())
                .parents(tipset1.key().clone())
                .miner_address(Address::new_id(0))
                .build()
                .unwrap()])
            .unwrap(),
        );
        assert_ne!(tipset1.key(), tipset3.key());
        assert_eq!(tipset3.parents(), tipset1.key());
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
