// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;
use std::time::Instant;

use crate::blocks::{Tipset, TipsetKey};
use crate::chain::TIPSET_LOOKUP_HAMT_BIT_WIDTH;
use crate::cli::subcommands::prompt_confirm;
use crate::cli_shared::{chain_path, delete_chain_data, read_config};
use crate::daemon::db_util::load_all_forest_cars;
use crate::db::car::{AnyCar, ManyCar};
use crate::db::db_engine::{db_root, open_db};
use crate::db::{
    BlockstoreWithWriteBuffer, CAR_DB_DIR_NAME, EthMappingsStore, HeaviestTipsetKeyProvider,
};
use crate::networks::NetworkChain;
use crate::prelude::*;
use crate::shim::policy::policy_constants::CHAIN_FINALITY;
use crate::utils::db::car_stream::CarStream;
use clap::Subcommand;
use fil_actors_shared::fvm_ipld_hamt::Hamt;
use indicatif::{ProgressBar, ProgressStyle};
use tokio_stream::StreamExt;

#[derive(Debug, Subcommand)]
pub enum DBCommands {
    /// Show DB stats
    Stats {
        /// Optional TOML file containing forest daemon configuration
        #[arg(short, long)]
        config: Option<PathBuf>,
        /// Optional chain, will override the chain section of configuration file if used
        #[arg(long)]
        chain: Option<NetworkChain>,
    },
    /// DB destruction
    Destroy {
        /// Answer yes to all forest-cli yes/no questions without prompting
        #[arg(long)]
        force: bool,
        /// Optional TOML file containing forest daemon configuration
        #[arg(short, long)]
        config: Option<PathBuf>,
        /// Optional chain, will override the chain section of configuration file if used
        #[arg(long)]
        chain: Option<NetworkChain>,
    },
    /// Import CAR files into the key-value store
    Import {
        /// Snapshot input paths. Supports `.car`, `.car.zst`, and `.forest.car.zst`.
        #[arg(num_args = 1.., required = true)]
        snapshot_files: Vec<PathBuf>,
        /// Filecoin network chain
        #[arg(long, required = true)]
        chain: NetworkChain,
        /// Optional path to the database folder that powers a Forest node
        #[arg(long)]
        db: Option<PathBuf>,
        /// Skip block validation
        #[arg(long)]
        skip_validation: bool,
    },
    /// Import a tipset lookup HAMT file into the key-value store
    ImportTipsetLookup {
        /// Snapshot input paths. Supports `.car`, `.car.zst`, and `.forest.car.zst`.
        #[arg(required = true)]
        snapshot: PathBuf,
        /// Filecoin network chain
        #[arg(long, required = true)]
        chain: NetworkChain,
        /// Optional path to the database folder that powers a Forest node
        #[arg(long)]
        db: Option<PathBuf>,
    },
}

impl DBCommands {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Stats { config, chain } => {
                use human_repr::HumanCount as _;

                let (_, config) = read_config(config.as_ref(), chain)?;

                let dir = db_root(&chain_path(&config))?;
                println!("Database path: {}", dir.display());
                let size = fs_extra::dir::get_size(dir).unwrap_or_default();
                println!("Database size: {}", size.human_count_bytes());
                Ok(())
            }
            Self::Destroy {
                force,
                config,
                chain,
            } => {
                let (_, config) = read_config(config.as_ref(), chain)?;

                let dir = chain_path(&config);
                if !dir.is_dir() {
                    println!(
                        "Aborted. Database path {} is not a valid directory",
                        dir.display()
                    );
                    return Ok(());
                }
                println!("Deleting {}", dir.display());
                if !force && !prompt_confirm() {
                    println!("Aborted.");
                    return Ok(());
                }
                match delete_chain_data(&config) {
                    Ok(_) => {
                        println!("Deleted {}", dir.display());
                        Ok(())
                    }
                    Err(err) => {
                        error!("{err:#}");
                        Ok(())
                    }
                }
            }
            Self::Import {
                snapshot_files,
                chain,
                db,
                skip_validation: no_validation,
            } => {
                const DB_WRITE_BUFFER_CAPACITY: usize = 10000;

                let db_root_path = if let Some(db) = db {
                    db
                } else {
                    let (_, config) = read_config(None, Some(chain.clone()))?;
                    db_root(&chain_path(&config))?
                };
                println!("Opening parity-db at {}", db_root_path.display());
                let db_writer = BlockstoreWithWriteBuffer::new_with_capacity(
                    open_db(db_root_path, &Default::default())?,
                    DB_WRITE_BUFFER_CAPACITY,
                );

                let pb = ProgressBar::new_spinner().with_style(
                    ProgressStyle::with_template("{spinner} {msg}")
                        .expect("indicatif template must be valid"),
                );
                pb.enable_steady_tick(std::time::Duration::from_millis(100));

                let mut total = 0;
                for snap in snapshot_files {
                    let mut car = CarStream::new_from_path(&snap).await?;
                    while let Some(b) = car.try_next().await? {
                        if !no_validation {
                            b.validate()?;
                        }
                        db_writer.put_keyed(&b.cid, &b.data)?;
                        total += 1;
                        let text = format!("{total} blocks imported");
                        pb.set_message(text);
                    }
                }
                drop(db_writer);
                pb.finish();
                Ok(())
            }
            Self::ImportTipsetLookup {
                snapshot,
                chain,
                db,
            } => {
                let start = Instant::now();
                println!("Loading tipset lookup hamt...");
                let hamt_car = AnyCar::try_from(snapshot.as_path())?;
                anyhow::ensure!(
                    hamt_car.header_v1().roots.len() == 1,
                    "hamt car should have a single root"
                );
                let hamt_root = *hamt_car.header_v1().roots.first();
                let hamt: Hamt<_, TipsetKey, ChainEpoch> =
                    Hamt::load_with_bit_width(&hamt_root, hamt_car, TIPSET_LOOKUP_HAMT_BIT_WIDTH)?;
                println!("Loaded tipset lookup hamt");

                let db_root_path = if let Some(db) = db {
                    db
                } else {
                    let (_, config) = read_config(None, Some(chain))?;
                    db_root(&chain_path(&config))?
                };
                println!("Opening parity-db at {}", db_root_path.display());
                let db = {
                    let forest_car_db_dir = db_root_path.join(CAR_DB_DIR_NAME);
                    let db = ManyCar::new(open_db(db_root_path, &Default::default())?);
                    load_all_forest_cars(&db, &forest_car_db_dir)?;
                    db
                };
                println!("Validating tipset lookup hamt...");
                hamt.for_each_cacheless(|&epoch, tsk| {
                    let ts = Tipset::load_required(&db, tsk)?;
                    anyhow::ensure!(
                        ts.epoch() == epoch,
                        "epochs do not match, {epoch} in hamt, {} in database",
                        ts.epoch()
                    );
                    anyhow::Ok(())
                })?;
                println!("Successfully validated tipset lookup hamt");
                let finality_boundary = HeaviestTipsetKeyProvider::heaviest_tipset_key(&db)?
                    .map(|tsk| Tipset::load_required(&db, &tsk).map(|ts| ts.epoch()))
                    .transpose()?
                    .map(|e| e - CHAIN_FINALITY)
                    .expect("chain head must be present in the database before importing a tipset lookup");
                let pb = ProgressBar::new_spinner().with_style(
                    ProgressStyle::with_template("{spinner} {pos} entries imported")
                        .expect("indicatif template must be valid"),
                );
                pb.enable_steady_tick(std::time::Duration::from_millis(100));
                hamt.for_each_cacheless(|&epoch, tsk| {
                    // skip entries that are not yet beyond the EC finality threshold
                    if epoch > finality_boundary {
                        return anyhow::Ok(());
                    }
                    // never overwrite an entry already recorded by the node
                    if db.tipset_key_by_epoch(epoch)?.is_some() {
                        return anyhow::Ok(());
                    }
                    db.set_tipset_key_at_epoch_raw(epoch, tsk)?;
                    pb.inc(1);
                    anyhow::Ok(())
                })?;
                drop(db);
                let total = pb.position();
                pb.finish();
                println!(
                    "Imported {total} tipset lookup entries, took {}",
                    humantime::format_duration(start.elapsed())
                );
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::store::index::tests::{
        chain_with_competing_tipsets_at_checkpoint, persist_tipset, tipset_child,
    };
    use crate::db::MemoryDB;
    use fil_actors_shared::fvm_ipld_hamt::Hamt;
    use std::sync::Arc;

    /// Regression test for #7860: import must not overwrite an entry the node already recorded.
    #[tokio::test]
    async fn import_tipset_lookup_does_not_overwrite_existing_entry() {
        let mem_db = Arc::new(MemoryDB::default());
        let (_genesis, partial, full, mut head) =
            chain_with_competing_tipsets_at_checkpoint(&mem_db);
        // Extend past CHAIN_FINALITY so epoch 20 is finalized (head - 900 >= 20 requires head >= 920).
        for epoch in head.epoch() + 1..=920 {
            let ts = tipset_child(&head, epoch);
            persist_tipset(&ts, &mem_db);
            head = ts;
        }

        // Export the chain to a CAR file for the paritydb to load.
        let temp_dir = tempfile::tempdir().unwrap();
        let db_root_path = temp_dir.path().join("db");
        let car_dir = db_root_path.join(CAR_DB_DIR_NAME);
        std::fs::create_dir_all(&car_dir).unwrap();
        let mut car = vec![];
        mem_db
            .export_forest_car_with_roots(head.key().to_cids(), &mut car)
            .await
            .unwrap();
        std::fs::write(car_dir.join("chain.forest.car.zst"), car).unwrap();

        let make_db = || {
            let db = ManyCar::new(open_db(db_root_path.clone(), &Default::default()).unwrap());
            load_all_forest_cars(&db, &car_dir).unwrap();
            Arc::new(db)
        };

        // Record the canonical entry for epoch 20 (the two-block tipset).
        let db = make_db();
        db.set_tipset_key_at_epoch_raw(full.epoch(), full.key())
            .unwrap();
        assert_eq!(
            db.tipset_key_by_epoch(20).unwrap().as_ref(),
            Some(full.key())
        );
        drop(db);

        // Build a lookup HAMT that maps epoch 20 to the one-block partial tipset.
        let hamt_store = Arc::new(MemoryDB::default());
        let mut hamt: Hamt<_, TipsetKey, ChainEpoch> =
            Hamt::new_with_bit_width(hamt_store.shallow_clone(), TIPSET_LOOKUP_HAMT_BIT_WIDTH);
        hamt.set(partial.epoch(), partial.key().clone()).unwrap();
        let root = hamt.flush().unwrap();
        let mut lookup_bytes = vec![];
        hamt_store
            .export_forest_car_with_roots(nunny::vec![root], &mut lookup_bytes)
            .await
            .unwrap();
        let lookup_path = temp_dir.path().join("lookup.forest.car.zst");
        std::fs::write(&lookup_path, lookup_bytes).unwrap();

        // Import should skip the entry because epoch 20 already has a canonical value.
        DBCommands::ImportTipsetLookup {
            snapshot: lookup_path,
            chain: NetworkChain::Calibnet,
            db: Some(db_root_path.clone()),
        }
        .run()
        .await
        .unwrap();

        let db = make_db();
        assert_eq!(
            db.tipset_key_by_epoch(20).unwrap().as_ref(),
            Some(full.key()),
            "import must not replace a canonical entry with an imported one",
        );
    }
}
