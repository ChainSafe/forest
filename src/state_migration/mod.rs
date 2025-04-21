// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::{
    Arc,
    atomic::{self, AtomicBool},
};

use crate::networks::{ChainConfig, Height, NetworkChain};
use crate::shim::clock::ChainEpoch;
use crate::shim::state_tree::StateRoot;
use ahash::{HashMap, HashMapExt};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use itertools::Itertools;
use parking_lot::RwLock;

pub(in crate::state_migration) mod common;
mod nv17;
mod nv18;
mod nv19;
mod nv21;
mod nv21fix;
mod nv21fix2;
mod nv22;
mod nv22fix;
mod nv23;
mod nv24;
mod nv25;
mod nv26fix;
mod type_migrations;

type RunMigration<DB> = fn(&ChainConfig, &Arc<DB>, &Cid, ChainEpoch) -> anyhow::Result<Cid>;

pub fn get_migrations<DB>(chain: &NetworkChain) -> Vec<(Height, RunMigration<DB>)>
where
    DB: Blockstore + Send + Sync,
{
    match chain {
        NetworkChain::Mainnet => {
            vec![
                (Height::Shark, nv17::run_migration::<DB>),
                (Height::Hygge, nv18::run_migration::<DB>),
                (Height::Lightning, nv19::run_migration::<DB>),
                (Height::Watermelon, nv21::run_migration::<DB>),
                (Height::Dragon, nv22::run_migration::<DB>),
                (Height::Waffle, nv23::run_migration::<DB>),
                (Height::TukTuk, nv24::run_migration::<DB>),
                (Height::Teep, nv25::run_migration::<DB>),
            ]
        }
        NetworkChain::Calibnet => {
            vec![
                (Height::Shark, nv17::run_migration::<DB>),
                (Height::Hygge, nv18::run_migration::<DB>),
                (Height::Lightning, nv19::run_migration::<DB>),
                (Height::Watermelon, nv21::run_migration::<DB>),
                (Height::WatermelonFix, nv21fix::run_migration::<DB>),
                (Height::WatermelonFix2, nv21fix2::run_migration::<DB>),
                (Height::Dragon, nv22::run_migration::<DB>),
                (Height::DragonFix, nv22fix::run_migration::<DB>),
                (Height::Waffle, nv23::run_migration::<DB>),
                (Height::TukTuk, nv24::run_migration::<DB>),
                (Height::Teep, nv25::run_migration::<DB>),
                (Height::TockFix, nv26fix::run_migration::<DB>),
            ]
        }
        NetworkChain::Butterflynet => {
            vec![(Height::Teep, nv25::run_migration::<DB>)]
        }
        NetworkChain::Devnet(_) => {
            vec![
                (Height::Shark, nv17::run_migration::<DB>),
                (Height::Hygge, nv18::run_migration::<DB>),
                (Height::Lightning, nv19::run_migration::<DB>),
                (Height::Watermelon, nv21::run_migration::<DB>),
                (Height::Dragon, nv22::run_migration::<DB>),
                (Height::Waffle, nv23::run_migration::<DB>),
                (Height::TukTuk, nv24::run_migration::<DB>),
                (Height::Teep, nv25::run_migration::<DB>),
                (Height::TockFix, nv26fix::run_migration::<DB>),
            ]
        }
    }
}

/// Run state migrations
pub fn run_state_migrations<DB>(
    epoch: ChainEpoch,
    chain_config: &ChainConfig,
    db: &Arc<DB>,
    parent_state: &Cid,
) -> anyhow::Result<Option<Cid>>
where
    DB: Blockstore + Send + Sync,
{
    let mappings = get_migrations(&chain_config.network);

    // Make sure bundle is defined.
    static BUNDLE_CHECKED: AtomicBool = AtomicBool::new(false);
    if !BUNDLE_CHECKED.load(atomic::Ordering::Relaxed) {
        BUNDLE_CHECKED.store(true, atomic::Ordering::Relaxed);
        for (info_height, info) in chain_config.height_infos.iter() {
            for (height, _) in &mappings {
                if height == info_height {
                    assert!(
                        info.bundle.is_some(),
                        "Actor bundle info for height {height} needs to be defined in `src/networks/mod.rs` to run state migration"
                    );
                    break;
                }
            }
        }
    }

    for (height, migrate) in mappings {
        if epoch == chain_config.epoch(height) {
            tracing::info!("Running {height} migration at epoch {epoch}");
            let start_time = std::time::Instant::now();
            let db = Arc::new(BlockstoreWithWriteBuffer::new(db.clone()));
            let new_state = migrate(chain_config, &db, parent_state, epoch)?;
            let elapsed = start_time.elapsed();
            // `new_state_actors` is the Go state migration output, log for comparision
            let new_state_actors = db
                .get_cbor::<StateRoot>(&new_state)
                .ok()
                .flatten()
                .map(|sr| format!("{}", sr.actors))
                .unwrap_or_default();
            if new_state != *parent_state {
                crate::utils::misc::reveal_upgrade_logo(height.into());
                tracing::info!(
                    "State migration at height {height}(epoch {epoch}) was successful, Previous state: {parent_state}, new state: {new_state}, new state actors: {new_state_actors}. Took: {elapsed}.",
                    elapsed = humantime::format_duration(elapsed)
                );
            } else {
                anyhow::bail!(
                    "State post migration at height {height} must not match. Previous state: {parent_state}, new state: {new_state}, new state actors: {new_state_actors}. Took {elapsed}.",
                    elapsed = humantime::format_duration(elapsed)
                );
            }

            return Ok(Some(new_state));
        }
    }

    Ok(None)
}

pub struct BlockstoreWithWriteBuffer<DB: Blockstore> {
    inner: DB,
    buffer: RwLock<HashMap<Cid, Vec<u8>>>,
    buffer_capacity: usize,
}

impl<DB: Blockstore> Blockstore for BlockstoreWithWriteBuffer<DB> {
    fn get(&self, k: &Cid) -> anyhow::Result<Option<Vec<u8>>> {
        if let Some(v) = self.buffer.read().get(k) {
            return Ok(Some(v.clone()));
        }
        self.inner.get(k)
    }

    fn has(&self, k: &Cid) -> anyhow::Result<bool> {
        Ok(self.buffer.read().contains_key(k) || self.inner.has(k)?)
    }

    fn put_keyed(&self, k: &Cid, block: &[u8]) -> anyhow::Result<()> {
        {
            let mut buffer = self.buffer.write();
            buffer.insert(*k, block.to_vec());
        }
        self.flush_buffer_if_needed()
    }
}

impl<DB: Blockstore> BlockstoreWithWriteBuffer<DB> {
    pub fn new(inner: DB) -> Self {
        Self::new_with_capacity(inner, 10000)
    }

    pub fn new_with_capacity(inner: DB, buffer_capacity: usize) -> Self {
        Self {
            inner,
            buffer_capacity,
            buffer: RwLock::new(HashMap::with_capacity(buffer_capacity)),
        }
    }

    fn flush_buffer(&self) -> anyhow::Result<()> {
        let records = {
            let mut buffer = self.buffer.write();
            buffer.drain().collect_vec()
        };
        self.inner.put_many_keyed(records)
    }

    fn flush_buffer_if_needed(&self) -> anyhow::Result<()> {
        if self.buffer.read().len() >= self.buffer_capacity {
            self.flush_buffer()
        } else {
            Ok(())
        }
    }
}

impl<DB: Blockstore> Drop for BlockstoreWithWriteBuffer<DB> {
    fn drop(&mut self) {
        if let Err(e) = self.flush_buffer() {
            tracing::warn!("{e}");
        }
    }
}

#[cfg(test)]
mod tests;
