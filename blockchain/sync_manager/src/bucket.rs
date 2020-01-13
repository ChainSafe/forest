// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use blocks::Tipset;

/// SyncBucket defines a bucket of tipsets to sync
#[derive(Clone, Default)]
struct SyncBucket<'a> {
    tips: Vec<&'a Tipset>,
}

impl<'a> SyncBucket<'a> {
    /// Constructor for tipset bucket
    fn new(tips: Vec<&'a Tipset>) -> SyncBucket {
        Self { tips }
    }
    /// heaviest_tipset returns the tipset with the max weight
    fn heaviest_tipset(&self) -> Option<&'a Tipset> {
        if self.tips.is_empty() {
            return None;
        }

        // return max value pointer
        self.tips.iter().max_by_key(|a| a.weight()).copied()
    }
    fn same_chain_as(&mut self, ts: &Tipset) -> bool {
        for t in self.tips.iter_mut() {
            // TODO Confirm that comparing keys will be sufficient on full tipset impl
            if ts.key() == t.key() || ts.key() == t.parents() || ts.parents() == t.key() {
                return true;
            }
        }

        false
    }
    fn add(&mut self, ts: &'a Tipset) {
        if !self.tips.iter().any(|t| *t == ts) {
            self.tips.push(ts);
        }
    }
}

/// Set of tipset buckets
#[derive(Default)]
pub(crate) struct SyncBucketSet<'a> {
    buckets: Vec<SyncBucket<'a>>,
}

impl<'a> SyncBucketSet<'a> {
    pub(crate) fn insert(&mut self, tipset: &'a Tipset) {
        for b in self.buckets.iter_mut() {
            if b.same_chain_as(tipset) {
                b.add(tipset);
                return;
            }
        }
        self.buckets.push(SyncBucket::new(vec![tipset]))
    }
    pub(crate) fn heaviest(&self) -> Option<&'a Tipset> {
        // Transform max values from each bucket into a Vec
        let vals: Vec<&'a Tipset> = self
            .buckets
            .iter()
            .filter_map(|b| b.heaviest_tipset())
            .collect();

        // Return the heaviest tipset bucket
        vals.iter().max_by_key(|b| b.weight()).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use address::Address;
    use blocks::{BlockHeader, TipSetKeys};
    use cid::{Cid, Codec};

    fn create_header(weight: u64, parent_bz: &[u8], cached_bytes: &[u8]) -> BlockHeader {
        let x = TipSetKeys {
            cids: vec![Cid::from_bytes_v1(Codec::DagCBOR, parent_bz)],
        };
        BlockHeader::builder()
            .parents(x)
            .cached_bytes(cached_bytes.to_vec()) // TODO change to however cached bytes are generated in future
            .miner_address(Address::new_id(0).unwrap())
            .bls_aggregate(vec![])
            .weight(weight)
            .build()
            .unwrap()
    }

    #[test]
    fn base_bucket_constructor() {
        SyncBucket::new(Vec::new());
    }

    #[test]
    fn heaviest_tipset() {
        let l_tip = Tipset::new(vec![create_header(1, b"", b"")]).unwrap();
        let h_tip = Tipset::new(vec![create_header(3, b"", b"")]).unwrap();

        // Test the comparison of tipsets
        let bucket = SyncBucket::new(vec![&l_tip, &h_tip]);
        assert_eq!(bucket.heaviest_tipset().unwrap().weight(), 3);
        assert_eq!(bucket.tips.len(), 2);

        // assert bucket with just one tipset still resolves
        let bucket = SyncBucket::new(vec![&l_tip]);
        assert_eq!(bucket.heaviest_tipset().unwrap().weight(), 1);
    }

    #[test]
    fn sync_bucket_inserts() {
        let mut set = SyncBucketSet::default();
        let tipset1 = Tipset::new(vec![create_header(1, b"1", b"1")]).unwrap();
        set.insert(&tipset1);
        assert_eq!(set.buckets.len(), 1);
        assert_eq!(set.buckets[0].tips.len(), 1);

        // Assert a tipset on non relating chain is put in another bucket
        let tipset2 = Tipset::new(vec![create_header(2, b"2", b"2")]).unwrap();
        set.insert(&tipset2);
        assert_eq!(set.buckets.len(), 2);
        assert_eq!(set.buckets[1].tips.len(), 1);

        // Assert a tipset connected to the first
        let tipset3 = Tipset::new(vec![create_header(3, b"1", b"1")]).unwrap();
        set.insert(&tipset3);
        assert_eq!(set.buckets.len(), 2);
        assert_eq!(set.buckets[0].tips.len(), 2);

        // Assert that tipsets that are already added are not added twice
        set.insert(&tipset1);
        assert_eq!(set.buckets.len(), 2);
        assert_eq!(set.buckets[0].tips.len(), 2);
    }
}
