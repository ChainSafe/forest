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
use crate::db::{BlockstoreWithWriteBuffer, CAR_DB_DIR_NAME, EthMappingsStore};
use crate::networks::NetworkChain;
use crate::prelude::*;
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

                let (_, config) = read_config(config.as_ref(), chain.clone())?;

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
                let (_, config) = read_config(config.as_ref(), chain.clone())?;

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
                        error!("{err}");
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
                    let (_, config) = read_config(None, Some(chain.clone()))?;
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
                let pb = ProgressBar::new_spinner().with_style(
                    ProgressStyle::with_template("{spinner} {pos} entries imported")
                        .expect("indicatif template must be valid"),
                );
                pb.enable_steady_tick(std::time::Duration::from_millis(100));
                hamt.for_each_cacheless(|&epoch, tsk| {
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
