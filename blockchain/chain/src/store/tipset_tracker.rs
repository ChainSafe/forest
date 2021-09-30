// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::sync::RwLock;
use log::{debug, warn};
use std::{collections::HashMap, sync::Arc};

use blocks::{BlockHeader, Tipset};
use cid::Cid;
use clock::ChainEpoch;
use ipld_blockstore::BlockStore;

use super::Error;

/// Tracks blocks by their height for the purpose of forming tipsets.
#[derive(Default)]
pub(crate) struct TipsetTracker<DB> {
    // TODO: look into optimizing https://github.com/ChainSafe/forest/issues/878
    entries: RwLock<HashMap<ChainEpoch, Vec<Cid>>>,
    db: Arc<DB>,
}

impl<DB: BlockStore> TipsetTracker<DB> {
    pub fn new(db: Arc<DB>) -> Self {
        Self {
            entries: Default::default(),
            db,
        }
    }

    /// Adds a block header to the tracker.
    pub async fn add(&self, header: &BlockHeader) {
        // TODO: consider only acquiring a writer lock when appending this header to the map,
        // in order to avoid holding the writer lock during the blockstore reads
        let mut map = self.entries.write().await;
        let cids = map.entry(header.epoch()).or_default();

        for cid in cids.iter() {
            if cid == header.cid() {
                debug!("tried to add block to tipset tracker that was already there");
                return;
            }
        }

        for cid in cids.iter() {
            // TODO: maybe cache the miner address to avoid having to do a blockstore lookup here
            if let Ok(Some(block)) = self.db.get::<BlockHeader>(cid) {
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

        cids.push(*header.cid());
    }

    /// Expands the given block header into the largest possible tipset by
    /// combining it with known blocks at the same height with the same parents.
    pub async fn expand(&self, header: BlockHeader) -> Result<Tipset, Error> {
        let epoch = header.epoch();
        let mut headers = vec![header];

        if let Some(entries) = self.entries.read().await.get(&epoch) {
            for cid in entries {
                if cid == headers[0].cid() {
                    continue;
                }

                // TODO: maybe cache the parents tipset keys to avoid having to do a blockstore lookup here
                let h = self
                    .db
                    .get::<BlockHeader>(&cid)
                    .ok()
                    .flatten()
                    .ok_or_else(|| {
                        Error::Other(format!(
                            "failed to load block ({}) for tipset expansion",
                            cid
                        ))
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
