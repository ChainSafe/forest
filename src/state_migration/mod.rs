// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::{
    Arc, LazyLock,
    atomic::{self, AtomicBool},
};

use ahash::HashMap;
use parking_lot::Mutex;

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
mod nv29;
mod type_migrations;

type RunMigration<DB> = fn(&ChainConfig, &DB, &Cid, ChainEpoch) -> anyhow::Result<Cid>;

pub fn get_migrations<DB>(chain: &NetworkChain) -> Vec<(Height, Option<RunMigration<DB>>)>
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
                (Height::Solstice, Some(nv29::run_migration::<DB>)),
            ]
        }
        NetworkChain::Butterflynet => {
            vec![(Height::Solstice, Some(nv29::run_migration::<DB>))]
        }
        NetworkChain::Devnet(_) => {
            vec![
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
                (Height::Solstice, Some(nv29::run_migration::<DB>)),
            ]
        }
    }
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

    // Make sure bundle is defined (skip unimplemented stubs).
    static BUNDLE_CHECKED: AtomicBool = AtomicBool::new(false);
    if !BUNDLE_CHECKED.load(atomic::Ordering::Relaxed) {
        BUNDLE_CHECKED.store(true, atomic::Ordering::Relaxed);
        for height in mappings
            .iter()
            .filter_map(|(height, migrate)| migrate.as_ref().map(|_| height))
        {
            let Some(info) = chain_config.height_infos.get(height) else {
                anyhow::bail!("Missing `HeightInfo` for migration height {height}");
            };
            anyhow::ensure!(
                info.bundle.is_some(),
                "Actor bundle info for height {height} needs to be defined in `src/networks/mod.rs` to run state migration"
            );
        }
    }

    for (height, migrate) in mappings {
        if epoch == chain_config.epoch(height) {
            let new_state = run_migration_once(epoch, parent_state, db, || {
                tracing::info!("Running {height} migration at epoch {epoch}");
                let start_time = std::time::Instant::now();
                let db = Arc::new(BlockstoreWithWriteBuffer::new_with_capacity(
                    db.shallow_clone(),
                    db_write_buffer,
                ));
                let migrate = migrate.ok_or_else(|| {
                    anyhow::anyhow!("Unimplemented state migration at height {height}")
                })?;
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
                Ok(new_state)
            })?;

            return Ok(Some(new_state));
        }
    }

    Ok(None)
}

type MigrationSlot = Arc<Mutex<Option<Cid>>>;

/// Tipsets sharing a parent (e.g. forks at the upgrade epoch) must not migrate the same state more than once, as mainnet migrations are expensive.
fn run_migration_once<DB: Blockstore>(
    epoch: ChainEpoch,
    parent_state: &Cid,
    db: &DB,
    migrate: impl FnOnce() -> anyhow::Result<Cid>,
) -> anyhow::Result<Cid> {
    static RESULTS: LazyLock<Mutex<HashMap<(ChainEpoch, Cid), MigrationSlot>>> =
        LazyLock::new(Default::default);

    let slot = RESULTS
        .lock()
        .entry((epoch, *parent_state))
        .or_default()
        .clone();
    let mut result = slot.lock();
    // The cached state may have been written to a different blockstore.
    if let Some(new_state) = *result
        && db.has(&new_state)?
    {
        return Ok(new_state);
    }
    let new_state = migrate()?;
    *result = Some(new_state);
    Ok(new_state)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod run_migration_once_tests {
    use super::*;
    use crate::db::MemoryDB;
    use crate::utils::db::CborStoreExt as _;
    use crate::utils::rand::random_cid;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn concurrent_callers_migrate_once() {
        let db = MemoryDB::default();
        let parent_state = random_cid();
        let new_state = db.put_cbor_default(&"new state").unwrap();
        let runs = AtomicUsize::new(0);

        let results: Vec<Cid> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    s.spawn(|| {
                        run_migration_once(42, &parent_state, &db, || {
                            runs.fetch_add(1, atomic::Ordering::Relaxed);
                            std::thread::sleep(std::time::Duration::from_millis(50));
                            Ok(new_state)
                        })
                        .unwrap()
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });

        assert_eq!(runs.load(atomic::Ordering::Relaxed), 1);
        assert!(results.iter().all(|c| *c == new_state));
    }

    #[test]
    fn failed_migration_is_retried() {
        let db = MemoryDB::default();
        let parent_state = random_cid();
        let new_state = db.put_cbor_default(&"new state").unwrap();

        assert!(run_migration_once(42, &parent_state, &db, || anyhow::bail!("boom")).is_err());
        assert_eq!(
            run_migration_once(42, &parent_state, &db, || Ok(new_state)).unwrap(),
            new_state
        );
    }

    #[test]
    fn reruns_when_result_missing_from_blockstore() {
        let parent_state = random_cid();
        let db1 = MemoryDB::default();
        let new_state = db1.put_cbor_default(&"new state").unwrap();
        run_migration_once(42, &parent_state, &db1, || Ok(new_state)).unwrap();

        let db2 = MemoryDB::default();
        let runs = AtomicUsize::new(0);
        run_migration_once(42, &parent_state, &db2, || {
            runs.fetch_add(1, atomic::Ordering::Relaxed);
            db2.put_cbor_default(&"new state")
        })
        .unwrap();
        assert_eq!(runs.load(atomic::Ordering::Relaxed), 1);
    }
}
