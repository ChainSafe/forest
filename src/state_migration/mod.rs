// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::atomic::{self, AtomicBool};

use crate::db::BlockstoreWithWriteBuffer;
use crate::networks::{ChainConfig, Height, NetworkChain};
use crate::prelude::*;
use crate::shim::clock::ChainEpoch;
use crate::shim::state_tree::StateRoot;
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
mod nv26fix;
mod nv27;
mod nv28;
mod type_migrations;

type RunMigration<DB> = fn(&ChainConfig, &DB, &Cid, ChainEpoch) -> anyhow::Result<Cid>;

/// Returns the upgrade-height registry for the given network. `Some(f)` entries are implemented
/// migrations; `None` entries are stubs for Lotus-`Expensive: true` heights that Forest has not
/// implemented, kept here so [`crate::networks::ChainConfig::has_expensive_fork_between`] can
/// refuse RPC calls spanning them.
/// Use [`get_migrations`] when only implemented migrations are needed.
pub fn get_all_migrations<DB>(chain: &NetworkChain) -> Vec<(Height, Option<RunMigration<DB>>)>
where
    DB: Blockstore + ShallowClone + Send + Sync,
{
    match chain {
        NetworkChain::Mainnet => {
            vec![
                (Height::Assembly, None),
                (Height::Trust, None),
                (Height::Turbo, None),
                (Height::Hyperdrive, None),
                (Height::Chocolate, None),
                (Height::OhSnap, None),
                (Height::Skyr, None),
                (Height::Shark, Some(nv17::run_migration::<DB>)),
                (Height::Hygge, Some(nv18::run_migration::<DB>)),
                (Height::Lightning, Some(nv19::run_migration::<DB>)),
                (Height::Watermelon, Some(nv21::run_migration::<DB>)),
                (Height::Dragon, Some(nv22::run_migration::<DB>)),
                (Height::Waffle, Some(nv23::run_migration::<DB>)),
                (Height::TukTuk, Some(nv24::run_migration::<DB>)),
                (Height::Teep, Some(nv25::run_migration::<DB>)),
                (Height::GoldenWeek, Some(nv27::run_migration::<DB>)),
                (Height::FireHorse, Some(nv28::run_migration::<DB>)),
            ]
        }
        NetworkChain::Calibnet => {
            vec![
                (Height::Assembly, None),
                (Height::Trust, None),
                (Height::Turbo, None),
                (Height::Hyperdrive, None),
                (Height::Chocolate, None),
                (Height::OhSnap, None),
                (Height::Skyr, None),
                (Height::Shark, Some(nv17::run_migration::<DB>)),
                (Height::Hygge, Some(nv18::run_migration::<DB>)),
                (Height::Lightning, Some(nv19::run_migration::<DB>)),
                (Height::Watermelon, Some(nv21::run_migration::<DB>)),
                (Height::WatermelonFix, Some(nv21fix::run_migration::<DB>)),
                (Height::WatermelonFix2, Some(nv21fix2::run_migration::<DB>)),
                (Height::Dragon, Some(nv22::run_migration::<DB>)),
                (Height::DragonFix, Some(nv22fix::run_migration::<DB>)),
                (Height::Waffle, Some(nv23::run_migration::<DB>)),
                (Height::TukTuk, Some(nv24::run_migration::<DB>)),
                (Height::Teep, Some(nv25::run_migration::<DB>)),
                (Height::TockFix, Some(nv26fix::run_migration::<DB>)),
                (Height::GoldenWeek, Some(nv27::run_migration::<DB>)),
                (Height::FireHorse, Some(nv28::run_migration::<DB>)),
            ]
        }
        NetworkChain::Butterflynet => {
            vec![
                (Height::Assembly, None),
                (Height::Trust, None),
                (Height::Turbo, None),
                (Height::Hyperdrive, None),
                (Height::Chocolate, None),
                (Height::OhSnap, None),
                (Height::Skyr, None),
                (Height::FireHorse, Some(nv28::run_migration::<DB>)),
            ]
        }
        NetworkChain::Devnet(_) => {
            vec![
                (Height::Assembly, None),
                (Height::Trust, None),
                (Height::Turbo, None),
                (Height::Hyperdrive, None),
                (Height::Chocolate, None),
                (Height::OhSnap, None),
                (Height::Skyr, None),
                (Height::Shark, Some(nv17::run_migration::<DB>)),
                (Height::Hygge, Some(nv18::run_migration::<DB>)),
                (Height::Lightning, Some(nv19::run_migration::<DB>)),
                (Height::Watermelon, Some(nv21::run_migration::<DB>)),
                (Height::Dragon, Some(nv22::run_migration::<DB>)),
                (Height::Waffle, Some(nv23::run_migration::<DB>)),
                (Height::TukTuk, Some(nv24::run_migration::<DB>)),
                (Height::Teep, Some(nv25::run_migration::<DB>)),
                (Height::GoldenWeek, Some(nv27::run_migration::<DB>)),
                (Height::FireHorse, Some(nv28::run_migration::<DB>)),
            ]
        }
    }
}

/// Returns the implemented migrations for the given network (i.e. [`get_all_migrations`] with
/// stub entries filtered out).
pub fn get_migrations<DB>(chain: &NetworkChain) -> Vec<(Height, RunMigration<DB>)>
where
    DB: Blockstore + ShallowClone + Send + Sync,
{
    get_all_migrations::<DB>(chain)
        .into_iter()
        .filter_map(|(h, migrate)| migrate.map(|f| (h, f)))
        .collect()
}

/// Run state migrations
pub fn run_state_migrations<DB>(
    epoch: ChainEpoch,
    chain_config: &ChainConfig,
    db: &DB,
    parent_state: &Cid,
) -> anyhow::Result<Option<Cid>>
where
    DB: Blockstore + ShallowClone + Send + Sync,
{
    // ~10MB RAM per 10k buffer
    let db_write_buffer = match std::env::var("FOREST_STATE_MIGRATION_DB_WRITE_BUFFER") {
        Ok(v) => v.parse().ok(),
        _ => None,
    }
    .unwrap_or(10000);
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
            let db = Arc::new(BlockstoreWithWriteBuffer::new_with_capacity(
                db.shallow_clone(),
                db_write_buffer,
            ));
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

#[cfg(test)]
mod tests;
