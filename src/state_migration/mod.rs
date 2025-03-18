// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::{
    atomic::{self, AtomicBool},
    Arc,
};

use crate::networks::{ChainConfig, Height, NetworkChain};
use crate::shim::clock::ChainEpoch;
use crate::shim::state_tree::StateRoot;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;

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
mod type_migrations;

type RunMigration<DB> = fn(&ChainConfig, &Arc<DB>, &Cid, ChainEpoch) -> anyhow::Result<Cid>;

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
    let mappings: Vec<(_, RunMigration<DB>)> = match chain_config.network {
        NetworkChain::Mainnet => {
            vec![
                (Height::Shark, nv17::run_migration::<DB>),
                (Height::Hygge, nv18::run_migration::<DB>),
                (Height::Lightning, nv19::run_migration::<DB>),
                (Height::Watermelon, nv21::run_migration::<DB>),
                (Height::Dragon, nv22::run_migration::<DB>),
                (Height::Waffle, nv23::run_migration::<DB>),
                (Height::TukTuk, nv24::run_migration::<DB>),
                // TODO(forest): https://github.com/ChainSafe/forest/issues/5041
                // (Height::Teep, nv25::run_migration::<DB>),
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
                // TODO(forest): To be re-enabled with FIP-0100 migration.
                // (Height::Teep, nv25::run_migration::<DB>),
            ]
        }
    };

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
            let new_state = migrate(chain_config, db, parent_state, epoch)?;
            let elapsed = start_time.elapsed().as_secs_f32();
            // `new_state_actors` is the Go state migration output, log for comparision
            let new_state_actors = db
                .get_cbor::<StateRoot>(&new_state)
                .ok()
                .flatten()
                .map(|sr| format!("{}", sr.actors))
                .unwrap_or_default();
            if new_state != *parent_state {
                crate::utils::misc::reveal_upgrade_logo(height.into());
                tracing::info!("State migration at height {height}(epoch {epoch}) was successful, Previous state: {parent_state}, new state: {new_state}, new state actors: {new_state_actors}. Took: {elapsed}s.");
            } else {
                anyhow:: bail!("State post migration at height {height} must not match. Previous state: {parent_state}, new state: {new_state}, new state actors: {new_state_actors}. Took {elapsed}s.");
            }

            return Ok(Some(new_state));
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests;
