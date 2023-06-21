// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{num::NonZeroUsize, sync::Arc};

use crate::blocks::{Tipset, TipsetKeys};
use crate::metrics;
use crate::shim::clock::ChainEpoch;
use crate::utils::io::ProgressBar;
use fvm_ipld_blockstore::Blockstore;
use log::info;
use lru::LruCache;
use nonzero_ext::nonzero;
use parking_lot::Mutex;

use crate::chain::{tipset_from_keys, Error, TipsetCache};

const DEFAULT_CHAIN_INDEX_CACHE_SIZE: NonZeroUsize = nonzero!(32usize << 10);

/// Configuration which sets the length of tipsets to skip in between each
/// cached entry.
const SKIP_LENGTH: ChainEpoch = 20;

// This module helps speed up boot times for forest by checkpointing previously
// seen tipsets from snapshots.
pub(super) mod checkpoint_tipsets {
    use std::str::FromStr;

    use crate::blocks::{Tipset, TipsetKeys};
    use crate::networks::NetworkChain;
    use ahash::HashSet;
    use cid::Cid;
    use serde::{Deserialize, Serialize};

    // Represents a static map of validated tipset hashes which helps to remove the
    // need to validate the tipset back to genesis if it has been validated
    // before, thereby reducing boot times. NB: Add desired tipset checkpoints
    // below this by using RPC command: forest-cli chain tipset-hash <cid keys>
    // and one can use forest-cli chain validate-tipset-checkpoints to validate
    // tipset hashes for entries that fall within the range of epochs in current
    // downloaded snapshot file.
    #[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
    struct KnownCheckpoints {
        calibnet: HashSet<String>,
        mainnet: HashSet<String>,
    }

    lazy_static::lazy_static! {
        static ref KNOWN_CHECKPOINTS: KnownCheckpoints = serde_yaml::from_str(include_str!("known_checkpoints.yaml")).unwrap();
    }

    type GenesisTipsetCids = TipsetKeys;

    pub(super) fn genesis_from_checkpoint_tipset(tsk: &TipsetKeys) -> Option<GenesisTipsetCids> {
        let key = tipset_hash(tsk);
        if KNOWN_CHECKPOINTS.mainnet.contains(&key) {
            return Some(TipsetKeys::new(vec![Cid::from_str(
                crate::networks::mainnet::GENESIS_CID,
            )
            .unwrap()]));
        }
        if KNOWN_CHECKPOINTS.calibnet.contains(&key) {
            return Some(TipsetKeys::new(vec![Cid::from_str(
                crate::networks::calibnet::GENESIS_CID,
            )
            .unwrap()]));
        }
        None
    }

    pub fn get_tipset_hashes(network: &NetworkChain) -> Option<HashSet<String>> {
        match network {
            NetworkChain::Mainnet => Some(KNOWN_CHECKPOINTS.mainnet.clone()),
            NetworkChain::Calibnet => Some(KNOWN_CHECKPOINTS.calibnet.clone()),
            NetworkChain::Devnet(_) => None, // skip and pass through if an unsupported network found
        }
    }

    // Validate that the genesis tipset matches our hard-coded values
    pub fn validate_genesis_cid(ts: &Tipset, network: &NetworkChain) -> bool {
        match network {
            NetworkChain::Mainnet => {
                ts.min_ticket_block().cid().to_string() == crate::networks::mainnet::GENESIS_CID
            }
            NetworkChain::Calibnet => {
                ts.min_ticket_block().cid().to_string() == crate::networks::calibnet::GENESIS_CID
            }
            NetworkChain::Devnet(_) => true, // skip and pass through if an unsupported network found
        }
    }

    pub fn tipset_hash(tsk: &TipsetKeys) -> String {
        let ts_bytes: Vec<_> = tsk.cids().iter().flat_map(|s| s.to_bytes()).collect();
        let tipset_keys_hash = blake2b_simd::blake2b(&ts_bytes).to_hex();
        tipset_keys_hash.to_string()
    }
}

/// `Lookback` entry to cache in the `ChainIndex`. Stores all relevant info when
/// doing `lookbacks`.
#[derive(Clone, PartialEq, Debug)]
pub(in crate::chain) struct LookbackEntry {
    tipset: Arc<Tipset>,
    parent_height: ChainEpoch,
    target_height: ChainEpoch,
    target: TipsetKeys,
}

/// Keeps look-back tipsets in cache at a given interval `skip_length` and can
/// be used to look-back at the chain to retrieve an old tipset.
pub(in crate::chain) struct ChainIndex<BS> {
    /// Cache of look-back entries to speed up lookup.
    skip_cache: Mutex<LruCache<TipsetKeys, Arc<LookbackEntry>>>,

    /// `Arc` reference tipset cache.
    ts_cache: Arc<TipsetCache>,

    /// `Blockstore` pointer needed to load tipsets from cold storage.
    db: BS,
}

impl<BS: Blockstore> ChainIndex<BS> {
    pub(in crate::chain) fn new(ts_cache: Arc<TipsetCache>, db: BS) -> Self {
        Self {
            skip_cache: Mutex::new(LruCache::new(DEFAULT_CHAIN_INDEX_CACHE_SIZE)),
            ts_cache,
            db,
        }
    }

    pub fn load_tipset(&self, tsk: &TipsetKeys) -> Result<Arc<Tipset>, Error> {
        tipset_from_keys(self.ts_cache.as_ref(), &self.db, tsk)
    }

    /// Loads tipset at `to` [`ChainEpoch`], loading from sparse cache and/or
    /// loading parents from the `blockstore`.
    pub(in crate::chain) fn get_tipset_by_height(
        &self,
        from: Arc<Tipset>,
        to: ChainEpoch,
    ) -> Result<Arc<Tipset>, Error> {
        if from.epoch() - to <= SKIP_LENGTH {
            return self.walk_back(from, to);
        }
        let total_size = from.epoch() - to;
        let pb = ProgressBar::new(total_size as u64);
        pb.message("Scanning blockchain ");
        pb.set_max_refresh_rate(Some(std::time::Duration::from_millis(500)));

        let rounded = self.round_down(from)?;

        let mut cur = rounded.key().clone();

        const MAX_COUNT: usize = 100;
        let mut counter = 0;
        loop {
            let entry = self.skip_cache.lock().get(&cur).cloned();
            let lbe = if let Some(cached) = entry {
                metrics::LRU_CACHE_HIT
                    .with_label_values(&[metrics::values::SKIP])
                    .inc();
                cached
            } else {
                metrics::LRU_CACHE_MISS
                    .with_label_values(&[metrics::values::SKIP])
                    .inc();
                self.fill_cache(std::mem::take(&mut cur))?
            };

            if to == 0 {
                if let Some(genesis_tipset_keys) =
                    checkpoint_tipsets::genesis_from_checkpoint_tipset(lbe.tipset.key())
                {
                    let tipset = tipset_from_keys(&self.ts_cache, &self.db, &genesis_tipset_keys)?;
                    pb.set(total_size as u64);
                    info!(
                        "Resolving genesis using checkpoint tipset at height: {}",
                        lbe.tipset.epoch()
                    );
                    return Ok(tipset);
                }
            }

            if lbe.tipset.epoch() == to || lbe.parent_height < to {
                return Ok(lbe.tipset.clone());
            } else if to > lbe.target_height {
                return self.walk_back(lbe.tipset.clone(), to);
            }
            let to_be_done = lbe.tipset.epoch() - to;
            // Don't show the progress bar if we're doing less than 10_000 units of work.
            if total_size > 10_000 {
                pb.set((total_size - to_be_done) as u64);
            }

            cur = lbe.target.clone();

            if counter == MAX_COUNT {
                counter = 0;
            } else {
                counter += 1;
            }
        }
    }

    /// Walks back from the tipset, ignoring the cached entries.
    /// This should only be used when the cache is checked to be invalidated.
    pub(in crate::chain) fn get_tipset_by_height_without_cache(
        &self,
        from: Arc<Tipset>,
        to: ChainEpoch,
    ) -> Result<Arc<Tipset>, Error> {
        self.walk_back(from, to)
    }

    /// Fills cache with look-back entry, and returns inserted entry.
    fn fill_cache(&self, tsk: TipsetKeys) -> Result<Arc<LookbackEntry>, Error> {
        let tipset = self.load_tipset(&tsk)?;

        if tipset.epoch() == 0 {
            return Ok(Arc::new(LookbackEntry {
                tipset,
                parent_height: 0,
                target_height: Default::default(),
                target: Default::default(),
            }));
        }

        let parent = self.load_tipset(tipset.parents())?;
        let r_height = self.round_height(tipset.epoch()) - SKIP_LENGTH;

        let parent_epoch = parent.epoch();
        let skip_target = if parent.epoch() < r_height {
            parent
        } else {
            self.walk_back(parent, r_height)?
        };

        let lbe = Arc::new(LookbackEntry {
            tipset,
            parent_height: parent_epoch,
            target_height: skip_target.epoch(),
            target: skip_target.key().clone(),
        });

        self.skip_cache.lock().put(tsk, lbe.clone());
        Ok(lbe)
    }

    /// Rounds height epoch to nearest sparse cache index epoch.
    fn round_height(&self, height: ChainEpoch) -> ChainEpoch {
        (height / SKIP_LENGTH) * SKIP_LENGTH
    }

    /// Gets the closest rounded sparse index and returns the loaded tipset at
    /// that index.
    fn round_down(&self, ts: Arc<Tipset>) -> Result<Arc<Tipset>, Error> {
        let target = self.round_height(ts.epoch());

        self.walk_back(ts, target)
    }

    /// Load parent tipsets until the `to` [`ChainEpoch`].
    fn walk_back(&self, from: Arc<Tipset>, to: ChainEpoch) -> Result<Arc<Tipset>, Error> {
        if to > from.epoch() {
            return Err(Error::Other(
                "Looking for tipset with height greater than start point".to_string(),
            ));
        }

        if to == from.epoch() {
            return Ok(from);
        }

        let mut ts = from;
        loop {
            let pts = self.load_tipset(ts.parents())?;

            if to > pts.epoch() {
                // Pts is lower than to epoch, return the tipset above that height
                return Ok(ts);
            }

            if to == pts.epoch() {
                return Ok(pts);
            }
            ts = pts;
        }
    }
}
