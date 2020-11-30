use std::{collections::HashMap, sync::Arc};

use blocks::{BlockHeader, Tipset};
use cid::Cid;
use clock::ChainEpoch;
use ipld_blockstore::BlockStore;

use super::Error;

/// Tracks blocks by their height for the purpose of forming tipsets.
#[derive(Default)]
pub struct TipsetTracker<DB> {
    entries: HashMap<ChainEpoch, Vec<Cid>>,
    db: Arc<DB>,
}

impl<DB: BlockStore> TipsetTracker<DB> {
    pub fn new(db: Arc<DB>) -> Self {
        Self {
            entries: HashMap::new(),
            db,
        }
    }

    /// Adds a block header to the tracker.
    pub fn add(&mut self, header: &BlockHeader) {
        let entries = self.entries.entry(header.epoch()).or_default();

        for cid in entries.iter() {
            if cid == header.cid() {
                log::debug!("tried to add block to tipset tracker that was already there");
                return;
            }

            if let Ok(Some(block)) = self.db.get::<BlockHeader>(cid) {
                if header.miner_address() == block.miner_address() {
                    log::warn!(
                        "Have multiple blocks from miner {} at height {} in our tipset cache {}-{}",
                        header.miner_address(),
                        header.epoch(),
                        header.cid(),
                        cid
                    );
                }
            }
        }

        entries.push(header.cid().clone());
    }

    /// Expands the given block header into the largest possible tipset by
    /// combining it with known blocks at the same height with the same parents.
    pub fn expand(&self, header: &BlockHeader) -> Result<Tipset, Error> {
        let mut headers = vec![header.clone()];

        if let Some(entries) = self.entries.get(&header.epoch()) {
            for cid in entries {
                if cid == header.cid() {
                    continue;
                }

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

                if h.parents() == header.parents() {
                    headers.push(h);
                }
            }
        }

        let ts = Tipset::new(headers)?;
        Ok(ts)
    }
}
