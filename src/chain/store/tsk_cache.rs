// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::hash::Hash;
use std::ops::Deref;
use std::sync::Arc;

use ahash::{AHashMap as HashMap, AHashSet as HashSet};
use anyhow::bail;
use tracing;

use super::index::ResolveNullTipset;
use crate::shim::clock::ChainEpoch;

type TskSetId = usize;

pub trait TipsetTrait<K> {
    fn epoch(&self) -> ChainEpoch;
    fn key(&self) -> &K;
}

#[derive(Debug, Default, Clone, PartialEq)]
struct EpochCache<K>
where
    K: Eq + Hash,
{
    pub(crate) setid_to_tskset: HashMap<TskSetId, HashSet<K>>,
    pub(crate) epoch_to_setid: HashMap<ChainEpoch, (Option<K>, TskSetId)>,
}

impl<K> EpochCache<K>
where
    K: Eq + Hash,
{
    /// Try to resolve `to_epoch`, backtracking from `from` Tipset.
    pub fn resolved_epoch<T, F>(
        &mut self,
        to_epoch: ChainEpoch,
        from: Arc<T>,
        resolve: ResolveNullTipset,
        mut load_parent: F,
    ) -> anyhow::Result<K>
    where
        T: Eq + Clone + Hash + TipsetTrait<K>,
        K: Eq + Clone + Hash,
        F: FnMut(&T) -> anyhow::Result<T>,
    {
        let opt = self.epoch_to_setid.get(&to_epoch).cloned();
        match opt {
            Some((Some(tsk), setid)) => {
                if let Some(tsk_set) = self.setid_to_tskset.get_mut(&setid) {
                    if tsk_set.contains(from.key()) {
                        tracing::info!("cache hit");
                        return Ok(tsk);
                    }
                    let mut curr: T = from.deref().clone();
                    let mut keys = vec![(curr.epoch(), curr.key().clone())];
                    let mut null_epochs = vec![];
                    let mut expected_epoch = curr.epoch() - 1;
                    let found = loop {
                        let parent = load_parent(&curr)?;
                        for e in expected_epoch..parent.epoch() {
                            null_epochs.push(e);
                        }
                        if tsk_set.contains(parent.key()) {
                            break true;
                        }
                        keys.push((parent.epoch(), parent.key().clone()));
                        if parent.epoch() == to_epoch {
                            break false;
                        }
                        curr = parent;
                        expected_epoch = curr.epoch() - 1;
                    };
                    if !null_epochs.is_empty() {
                        tracing::info!("null epochs: {:?}:", null_epochs);
                    }
                    if found {
                        tracing::info!(
                            "setid {}: extending cache with {} tipset keys",
                            setid,
                            keys.len()
                        );
                        for (_, tsk) in keys.iter() {
                            tsk_set.insert(tsk.clone());
                        }
                        for (epoch, tsk) in keys.into_iter() {
                            self.epoch_to_setid.insert(epoch, (Some(tsk), setid));
                        }
                        tracing::info!("cache hit through extension");
                        return Ok(tsk);
                    } else {
                        let setid = self.setid_to_tskset.len();
                        tracing::info!(
                            "setid {}: creating cache with {} tipset keys",
                            setid,
                            keys.len()
                        );
                        let mut tsk_set = HashSet::default();
                        for (_, tsk) in keys.iter() {
                            tsk_set.insert(tsk.clone());
                        }
                        for (epoch, tsk) in keys.into_iter() {
                            self.epoch_to_setid.insert(epoch, (Some(tsk), setid));
                        }
                        self.setid_to_tskset.insert(setid, tsk_set);

                        bail!("epoch {} not found", to_epoch);
                    }
                } else {
                    bail!("set {} not found", setid);
                }
            }
            Some((None, _)) => {
                tracing::info!("null epoch found");
                match resolve {
                    ResolveNullTipset::TakeOlder => {
                        self.resolved_epoch(to_epoch - 1, from, resolve, load_parent)
                    }
                    ResolveNullTipset::TakeNewer => {
                        self.resolved_epoch(to_epoch + 1, from, resolve, load_parent)
                    }
                }
            }
            None => {
                // empty
                let mut curr: T = from.deref().clone();
                let mut keys = vec![(curr.epoch(), curr.key().clone())];
                let mut null_epochs = vec![];
                let mut expected_epoch = curr.epoch() - 1;
                let found = loop {
                    let parent = load_parent(&curr)?;
                    for e in expected_epoch..parent.epoch() {
                        null_epochs.push(e);
                    }
                    keys.push((parent.epoch(), parent.key().clone()));
                    if parent.epoch() == to_epoch {
                        break Some(parent.key().clone());
                    }
                    if parent.epoch() == 0 {
                        break None;
                    }
                    curr = parent;
                    expected_epoch = curr.epoch() - 1;
                };
                if !null_epochs.is_empty() {
                    tracing::info!("null epochs: {:?}:", null_epochs);
                }
                if let Some(tsk) = found {
                    let setid = self.setid_to_tskset.len();
                    tracing::info!(
                        "setid {}: creating cache with {} tipset keys",
                        setid,
                        keys.len()
                    );
                    let mut tsk_set = HashSet::default();
                    for (_, tsk) in keys.iter() {
                        tsk_set.insert(tsk.clone());
                    }
                    for (epoch, tsk) in keys.into_iter() {
                        self.epoch_to_setid.insert(epoch, (Some(tsk), setid));
                    }
                    self.setid_to_tskset.insert(setid, tsk_set);

                    tracing::info!("cache hit through creation");
                    return Ok(tsk);
                } else {
                    bail!("epoch {} not found", to_epoch);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EpochCache;
    use super::*;

    /// The minimal `Tipset` structure for testing.
    #[derive(Debug, Clone, Eq)]
    pub struct MockTipset {
        epoch: ChainEpoch,
        key: String,
        parent: Option<Arc<MockTipset>>,
    }

    impl MockTipset {
        pub fn new(
            epoch: ChainEpoch,
            key: impl Into<String>,
            parent: Option<Arc<MockTipset>>,
        ) -> Self {
            Self {
                epoch,
                key: key.into(),
                parent,
            }
        }
    }

    impl TipsetTrait<String> for MockTipset {
        fn epoch(&self) -> ChainEpoch {
            self.epoch
        }

        fn key(&self) -> &String {
            &self.key
        }
    }

    impl PartialEq for MockTipset {
        fn eq(&self, other: &Self) -> bool {
            self.key == other.key
        }
    }

    impl Hash for MockTipset {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.key.hash(state);
        }
    }

    #[test]
    fn test_tsk_cache() {
        tracing_subscriber::fmt::init();

        let load_parent = |ts: &MockTipset| {
            ts.parent
                .as_ref()
                .map(|arc| arc.deref().clone())
                .ok_or_else(|| anyhow::anyhow!("no parent found"))
        };

        let mut cache: EpochCache<String> = EpochCache::default();

        // Build a short chain: 0 <- 1 <- 2 <- 3 <- 4
        let ts0 = Arc::new(MockTipset::new(0, "tsk0", None));
        let ts1 = Arc::new(MockTipset::new(1, "tsk1", Some(ts0.clone())));
        let ts2 = Arc::new(MockTipset::new(2, "tsk2", Some(ts1.clone())));
        let ts3 = Arc::new(MockTipset::new(3, "tsk3", Some(ts2.clone())));
        let _ts4 = Arc::new(MockTipset::new(4, "tsk4", Some(ts3.clone())));

        // tsk1 should be found and the cache is filled
        let result =
            cache.resolved_epoch(1, ts3.clone(), ResolveNullTipset::TakeOlder, load_parent);
        assert_eq!(result.unwrap(), "tsk1");
        assert_eq!(cache.setid_to_tskset.get(&0).unwrap().len(), 3);

        // tsk1 should be found and the cache is left untouched
        let cloned_cache = cache.clone();
        let result =
            cache.resolved_epoch(1, ts3.clone(), ResolveNullTipset::TakeOlder, load_parent);
        assert_eq!(result.unwrap(), "tsk1");
        assert_eq!(cache.setid_to_tskset.get(&0).unwrap().len(), 3);
        assert_eq!(cloned_cache, cache);

        dbg!(cache);
    }
}
