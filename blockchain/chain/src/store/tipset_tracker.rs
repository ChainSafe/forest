// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use log::{debug, warn};
use std::collections::HashMap;

use cid::Cid;
use forest_blocks::{BlockHeader, Tipset};
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::clock::ChainEpoch;
use parking_lot::Mutex;

use super::Error;

/// Tracks blocks by their height for the purpose of forming tipsets.
#[derive(Default)]
pub(crate) struct TipsetTracker<DB> {
    entries: Mutex<HashMap<ChainEpoch, Vec<Cid>>>,
    db: DB,
}

impl<DB: Blockstore> TipsetTracker<DB> {
    pub fn new(db: DB) -> Self {
        Self {
            entries: Default::default(),
            db,
        }
    }

    /// Adds a block header to the tracker.
    pub fn add(&self, header: &BlockHeader) {
        let cids = {
            let mut map = self.entries.lock();
            let cids = map.entry(header.epoch()).or_default();
            if cids.contains(header.cid()) {
                debug!("tried to add block to tipset tracker that was already there");
                return;
            }
            cids.push(*header.cid());
            cids.clone()
        };

        // XXX: What is this code supposed to do? ~Lemmih
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
