// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! This module implements a garbage collector that transforms parity-db into an effective
//! lite snapshot then purges parity-db

use crate::blocks::Tipset;
use crate::cid_collections::CidHashSet;
use crate::cli_shared::chain_path;
use crate::db::SettingsStoreExt;
use crate::db::{
    CAR_DB_DIR_NAME, SettingsStore,
    car::forest::FOREST_CAR_FILE_EXTENSION,
    db_engine::{DbConfig, db_root},
    parity_db::{DbColumn, ParityDb},
};
use anyhow::Context as _;
use fvm_ipld_blockstore::Blockstore;
use parking_lot::RwLock;
use sha2::Sha256;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

pub struct SnapshotGarbageCollector<DB> {
    db_root_dir: PathBuf,
    car_db_dir: PathBuf,
    recent_state_roots: i64,
    db_config: DbConfig,
    running: AtomicBool,
    reboot_tx: flume::Sender<()>,
    exported_chain_head: RwLock<Option<Tipset>>,
    blessed_lite_snapshot: RwLock<Option<PathBuf>>,
    db: RwLock<Option<Arc<DB>>>,
}

impl<DB> SnapshotGarbageCollector<DB>
where
    DB: Blockstore + SettingsStore + Send + Sync + 'static,
{
    pub fn new(config: &crate::Config) -> anyhow::Result<(Self, flume::Receiver<()>)> {
        let chain_data_path = chain_path(config);
        let db_root_dir = db_root(&chain_data_path)?;
        let car_db_dir = db_root_dir.join(CAR_DB_DIR_NAME);
        let recent_state_roots = config.sync.recent_state_roots;
        let (reboot_tx, reboot_rx) = flume::bounded(1);
        Ok((
            Self {
                db_root_dir,
                car_db_dir,
                recent_state_roots,
                db_config: config.db_config().clone(),
                running: AtomicBool::new(false),
                reboot_tx,
                exported_chain_head: RwLock::new(None),
                blessed_lite_snapshot: RwLock::new(None),
                db: RwLock::new(None),
            },
            reboot_rx,
        ))
    }

    pub fn running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    pub fn set_db(&self, db: Arc<DB>) {
        *self.db.write() = Some(db);
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        if self.running() {
            anyhow::bail!("snap gc has already been running");
        }
        if let Err(e) = self.export_snapshot().await {
            self.running.store(false, Ordering::Relaxed);
            Err(e)
        } else {
            Ok(())
        }
    }

    async fn export_snapshot(&self) -> anyhow::Result<()> {
        let db = self.db.read().clone().context("db not yet initialzied")?;
        self.running.store(true, Ordering::Relaxed);
        tracing::info!("exporting lite snapshot");
        let temp_path = tempfile::NamedTempFile::new_in(&self.car_db_dir)?.into_temp_path();
        let file = tokio::fs::File::create(&temp_path).await?;
        let (head_ts, _) = crate::chain::export_from_head::<Sha256>(
            db,
            self.recent_state_roots,
            file,
            CidHashSet::default(),
            true,
        )
        .await?;
        let target_path = self
            .car_db_dir
            .join(format!("lite_{}.forest.car.zst", head_ts.epoch()));
        temp_path.persist(&target_path)?;
        tracing::info!("exported lite snapshot at {}", target_path.display());
        *self.exported_chain_head.write() = Some(head_ts);
        *self.blessed_lite_snapshot.write() = Some(target_path);

        if let Err(e) = self.reboot_tx.send(()) {
            tracing::warn!("{e}");
        }
        Ok(())
    }

    pub async fn cleanup_before_reboot(&self) {
        if let Err(e) = self.cleanup_before_reboot_inner().await {
            tracing::warn!("{e}");
        }
        self.running.store(false, Ordering::Relaxed);
    }

    async fn cleanup_before_reboot_inner(&self) -> anyhow::Result<()> {
        tracing::info!("cleaning up db before rebooting");
        {
            if let (Some(db), Some(head)) =
                (self.db.write().take(), &*self.exported_chain_head.read())
            {
                SettingsStoreExt::write_obj(&db, crate::db::setting_keys::HEAD_KEY, head.key())?;
            }
        }
        if let Some(blessed_lite_snapshot) = { self.blessed_lite_snapshot.read().clone() } {
            let mut opts = ParityDb::to_options(self.db_root_dir.clone(), &self.db_config);
            for col in [
                DbColumn::GraphDagCborBlake2b256 as u8,
                DbColumn::GraphFull as u8,
            ] {
                let start = Instant::now();
                tracing::info!("pruning parity-db column {col}...");
                loop {
                    match parity_db::Db::reset_column(&mut opts, col, None) {
                        Ok(_) => break,
                        Err(_) => {
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
                tracing::info!(
                    "pruned parity-db column {col}, took {}",
                    humantime::format_duration(start.elapsed())
                );
            }

            for car_to_remove in walkdir::WalkDir::new(&self.car_db_dir)
                .max_depth(1)
                .into_iter()
                .filter_map(|entry| {
                    if let Ok(entry) = entry {
                        if entry.path() != blessed_lite_snapshot.as_path() {
                            if let Some(filename) = entry.file_name().to_str() {
                                if filename.ends_with(FOREST_CAR_FILE_EXTENSION) {
                                    return Some(entry.into_path());
                                }
                            }
                        }
                    }
                    None
                })
            {
                match std::fs::remove_file(&car_to_remove) {
                    Ok(_) => {
                        tracing::info!("deleted car db at {}", car_to_remove.display());
                    }
                    Err(e) => {
                        tracing::warn!(
                            "failed to delete car db at {}: {e}",
                            car_to_remove.display()
                        );
                    }
                }
            }
        }
        Ok(())
    }
}
