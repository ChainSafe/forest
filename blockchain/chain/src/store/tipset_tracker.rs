// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use forest_blocks::{BlockHeader, Tipset};
use forest_networks::ChainConfig;
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::clock::ChainEpoch;
use log::{debug, warn};
use parking_lot::Mutex;
use std::{collections::BTreeMap, sync::Arc};

use super::Error;

/// Tracks blocks by their height for the purpose of forming tipsets.
#[derive(Default)]
pub(crate) struct TipsetTracker<DB> {
    entries: Mutex<BTreeMap<ChainEpoch, Vec<Cid>>>,
    db: DB,
    chain_config: Arc<ChainConfig>,
}

impl<DB: Blockstore> TipsetTracker<DB> {
    pub fn new(db: DB, chain_config: Arc<ChainConfig>) -> Self {
        Self {
            entries: Default::default(),
            db,
            chain_config,
        }
    }

    /// Adds a block header to the tracker.
    pub fn add(&self, header: &BlockHeader) {
        let mut map_lock = self.entries.lock();
        let cids = map_lock.entry(header.epoch()).or_default();
        if cids.contains(header.cid()) {
            debug!("tried to add block to tipset tracker that was already there");
            return;
        }
        let cids_to_verify = cids.to_owned();
        cids.push(*header.cid());
        drop(map_lock);

        self.check_multiple_blocks_from_same_miner(&cids_to_verify, header);
        self.prune_entries(header.epoch());
    }

    /// Checks if there are multiple blocks from the same miner at the same height.
    ///
    /// This should never happen. Something is weird as it's against the protocol rules for a
    /// miner to produce multiple blocks at the same height.
    fn check_multiple_blocks_from_same_miner(&self, cids: &[Cid], header: &BlockHeader) {
        for cid in cids.iter() {
            // TODO: maybe cache the miner address to avoid having to do a `blockstore` lookup here
            if let Ok(Some(block)) = self.db.get_obj::<BlockHeader>(cid) {
                if header.miner_address() == block.miner_address() {
                    warn!(
                        "Have multiple blocks from miner {} at height {} in our tipset cache {}-{}",
                        header.miner_address(),
                        header.epoch(),
                        header.cid(),
                        cid
                    );
                }
            }
        }
    }

    /// Deletes old entries in the `TipsetTracker` that are past the chain finality.
    fn prune_entries(&self, header_epoch: ChainEpoch) {
        let cut_off_epoch = header_epoch - self.chain_config.policy.chain_finality;
        let mut entries = self.entries.lock();
        let mut finality_entries = entries.split_off(&cut_off_epoch);
        debug!(
            "Cleared {} entries, cut off at {}",
            entries.len(),
            cut_off_epoch,
        );
        std::mem::swap(&mut finality_entries, &mut entries);
    }

    /// Expands the given block header into the largest possible tipset by
    /// combining it with known blocks at the same height with the same parents.
    pub fn expand(&self, header: BlockHeader) -> Result<Tipset, Error> {
        let epoch = header.epoch();
        let mut headers = vec![header];

        if let Some(entries) = self.entries.lock().get(&epoch).cloned() {
            for cid in entries {
                if &cid == headers[0].cid() {
                    continue;
                }

                // TODO: maybe cache the parents tipset keys to avoid having to do a `blockstore` lookup here
                let h = self
                    .db
                    .get_obj::<BlockHeader>(&cid)
                    .ok()
                    .flatten()
                    .ok_or_else(|| {
                        Error::Other(format!("failed to load block ({cid}) for tipset expansion"))
                    })?;

                if h.parents() == headers[0].parents() {
                    headers.push(h);
                }
            }
        }

        let ts = Tipset::new(headers)?;
        Ok(ts)
    }
}

#[cfg(test)]
mod test {
    use forest_db::MemoryDB;

    use super::*;

    #[test]
    fn ensure_tipset_is_bounded() {
        let db = MemoryDB::default();
        let chain_config = Arc::new(ChainConfig::default());

        let head_epoch = 2023;

        let entries = BTreeMap::from([
            (head_epoch - chain_config.policy.chain_finality - 3, vec![]),
            (head_epoch - chain_config.policy.chain_finality - 1, vec![]),
            (head_epoch - chain_config.policy.chain_finality, vec![]),
            (head_epoch - chain_config.policy.chain_finality + 1, vec![]),
            (head_epoch - chain_config.policy.chain_finality + 3, vec![]),
        ]);
        let tipset_tracker = TipsetTracker {
            db,
            chain_config: chain_config.clone(),
            entries: Mutex::new(entries),
        };

        tipset_tracker.prune_entries(head_epoch);

        let keys = tipset_tracker
            .entries
            .lock()
            .keys()
            .cloned()
            .collect::<Vec<_>>();

        assert_eq!(
            keys,
            vec![
                head_epoch - chain_config.policy.chain_finality,
                head_epoch - chain_config.policy.chain_finality + 1,
                head_epoch - chain_config.policy.chain_finality + 3,
            ]
        );
    }
}
