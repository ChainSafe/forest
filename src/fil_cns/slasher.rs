// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::fvm_shared_latest::consensus::ConsensusFaultType;
use crate::utils::db::CborStoreExt as _;
use anyhow::{Context, Result};
use cid::Cid;
use fjall::{Config, PartitionCreateOptions, PartitionHandle};
use fvm_ipld_blockstore::Blockstore;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use crate::blocks::CachingBlockHeader;
use crate::state_manager::StateManager;

pub struct Slasher {
    by_epoch: PartitionHandle,
    by_parents: PartitionHandle,
}

impl Slasher {
    pub fn new() -> Result<Self> {
        let data_dir = env::var("ConsensusFaultReporterDataDir")
            .map(PathBuf::from)
            .expect("ConsensusFaultReporterDataDir must be set");

        let db = Config::new(data_dir).open()?;
        let by_epoch = db.open_partition("by_epoch", PartitionCreateOptions::default())?;
        let by_parents = db.open_partition("by_parents", PartitionCreateOptions::default())?;
        Ok(Slasher {
            by_epoch,
            by_parents,
        })
    }

    fn mined_block(&self, bh: &CachingBlockHeader, parent_epoch: i64) -> Result<(Cid, bool)> {
        // Double-fork mining check
        let epoch_key = format!("{}/{}", bh.miner_address, bh.epoch).into_bytes();
        let (witness, is_fault) = check_fault(&self.by_epoch, &epoch_key, bh)?;
        if is_fault {
            return Ok((witness, true));
        }

        // Time-offset mining check
        let parents_key = format!("{}/{}", bh.miner_address, bh.parents).into_bytes();
        let (witness, is_fault) = check_fault(&self.by_parents, &parents_key, bh)?;
        if is_fault {
            return Ok((witness, true));
        }

        // Parent-grinding check
        let parent_epoch_key = format!("{}/{}", bh.miner_address, parent_epoch).into_bytes();
        if self.by_epoch.contains_key(&parent_epoch_key)? {
            let cid_bytes = self
                .by_epoch
                .get(&parent_epoch_key)?
                .context("expected CID bytes in parent epoch, but found None")?;
            let parent_cid = Cid::try_from(cid_bytes.as_ref())
                .context("failed to decode CID from bytes in parent epoch")?;

            if !bh.parents.contains(parent_cid) {
                return Ok((parent_cid, true)); // Parent-grinding fault
            }
        }

        // No faults found – store CID for future checks
        self.by_epoch.insert(epoch_key, bh.cid().to_bytes())?;
        self.by_parents.insert(parents_key, bh.cid().to_bytes())?;

        Ok((Cid::default(), false))
    }
}

fn check_fault(
    store: &PartitionHandle,
    key: &[u8],
    bh: &CachingBlockHeader,
) -> Result<(Cid, bool)> {
    // Check if the key exists
    if store.contains_key(key)? {
        // Retrieve the value (CID bytes) stored under the key
        let cid_bytes = store
            .get(key)?
            .context("expected CID bytes, but found None")?;

        // Decode the CID
        let other_cid =
            Cid::try_from(cid_bytes.as_ref()).context("failed to parse CID from bytes")?;

        // Compare the CID with the one from the block header
        if other_cid == *bh.cid() {
            Ok((Cid::default(), false)) // Not a fault: same CID
        } else {
            Ok((other_cid, true)) // Fault: different CID reported
        }
    } else {
        Ok((Cid::default(), false)) // No entry = no fault
    }
}

pub fn check_consensus_fault<DB: Blockstore + Send + Sync + 'static>(
    state_manager: Arc<StateManager<DB>>,
    slasher: &Slasher,
    block_b: &CachingBlockHeader,
) -> Result<(
    Option<ConsensusFaultType>,
    Option<CachingBlockHeader>,
    Option<CachingBlockHeader>,
)> {
    let block_c: CachingBlockHeader = state_manager
        .chain_store()
        .blockstore()
        .get_cbor_required(block_b.parents.to_cids().first())?;
    let (block_a_cid, fault) = slasher.mined_block(block_b, block_c.epoch)?;

    if fault {
        let block_a: CachingBlockHeader = state_manager
            .chain_store()
            .blockstore()
            .get_cbor_required(&block_a_cid)?;
        if block_a.epoch == block_b.epoch && block_b.parents != block_a.parents {
            return Ok((
                Some(ConsensusFaultType::DoubleForkMining),
                Some(block_a),
                None,
            ));
        }
        if block_b.parents == block_a.parents && block_a.epoch != block_b.epoch {
            return Ok((
                Some(ConsensusFaultType::TimeOffsetMining),
                Some(block_a),
                None,
            ));
        }
        if block_a.parents == block_c.parents
            && block_a.epoch == block_c.epoch
            && block_b.parents.contains(*block_c.cid())
            && !block_b.parents.contains(*block_a.cid())
        {
            return Ok((
                Some(ConsensusFaultType::ParentGrinding),
                Some(block_a),
                Some(block_c),
            ));
        }
    }
    Ok((None, None, None))
}
