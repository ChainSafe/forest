// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::{
    atomic::{self, AtomicBool},
    Arc,
};

use cid::Cid;
use forest_networks::{ChainConfig, Height};
use forest_shim::clock::ChainEpoch;
use forest_utils::misc::reveal_five_trees;
use fvm_ipld_blockstore::Blockstore;

pub(crate) mod common;
mod nv18;
mod nv19;
mod type_migrations;

type RunMigration<DB> = fn(&ChainConfig, &DB, &Cid, ChainEpoch) -> anyhow::Result<Cid>;

/// Run state migrations
pub fn run_state_migrations<DB>(
    epoch: ChainEpoch,
    chain_config: &Arc<ChainConfig>,
    db: &DB,
    parent_state: &Cid,
) -> anyhow::Result<Option<Cid>>
where
    DB: 'static + Blockstore + Clone + Send + Sync,
{
    let mappings: [(_, RunMigration<DB>); 2] = [
        (Height::Hygge, nv18::run_migration::<DB>),
        (Height::Lightning, nv19::run_migration::<DB>),
    ];

    // Make sure bundle is defined.
    static BUNDLE_CHECKED: AtomicBool = AtomicBool::new(false);
    if !BUNDLE_CHECKED.load(atomic::Ordering::Relaxed) {
        BUNDLE_CHECKED.store(true, atomic::Ordering::Relaxed);
        for info in &chain_config.height_infos {
            for (height, _) in &mappings {
                if height == &info.height {
                    anyhow::ensure!(
                        info.bundle.is_some(),
                        "Actor bundle info for height {height} needs to be defined in `networks/src/lib.rs` to run state migration"
                    );
                    break;
                }
            }
        }
    }

    for (height, migrate) in mappings {
        if epoch == chain_config.epoch(height) {
            log::info!("Running {height} migration at epoch {epoch}");
            let start_time = std::time::Instant::now();
            let new_state = migrate(chain_config, db, parent_state, epoch)?;
            let elapsed = start_time.elapsed().as_secs_f32();
            if new_state != *parent_state {
                reveal_five_trees();
                log::info!("State migration at height {height} was successful, took: {elapsed}s");
            } else {
                anyhow:: bail!("State post migration at height {height} must not match. Previous state: {parent_state}, new state: {new_state}. Took {elapsed}s");
            }

            return Ok(Some(new_state));
        }
    }

    Ok(None)
}
