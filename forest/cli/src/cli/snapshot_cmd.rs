// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{fs, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{bail, Context as _};
use canonical_path::CanonicalPathBuf;
use chrono::Utc;
use clap::Subcommand;
use dialoguer::{theme::ColorfulTheme, Confirm};
use forest_blocks::{tipset_keys_json::TipsetKeysJson, Tipset, TipsetKeys};
use forest_chain::ChainStore;
use forest_cli_shared::cli::default_snapshot_dir;
use forest_db::db_engine::{db_root, open_proxy_db};
use forest_genesis::{forest_load_car, read_genesis_header};
use forest_ipld::{recurse_links_hash, CidHashSet};
use forest_networks::NetworkChain;
use forest_rpc_api::{chain_api::ChainExportParams, progress_api::GetProgressType};
use forest_rpc_client::{chain_ops::*, progress_ops::get_progress};
use forest_utils::{
    io::{parser::parse_duration, ProgressBar},
    net::get_fetch_progress_from_file,
    retry, RetryArgs,
};
use fvm_shared::clock::ChainEpoch;
use tempfile::TempDir;

use super::*;
use crate::cli::{cli_error_and_die, handle_rpc_err};

#[derive(Debug, Subcommand)]
pub enum SnapshotCommands {
    /// Export a snapshot of the chain to `<output_path>`
    Export {
        /// Snapshot output path. Default to
        /// `forest_snapshot_{chain}_{year}-{month}-{day}_height_{epoch}.car.
        /// zst`.
        #[arg(short, default_value = ".", verbatim_doc_comment)]
        output_path: PathBuf,
        /// Skip creating the checksum file.
        #[arg(long)]
        skip_checksum: bool,
        /// Don't write the archive.
        #[arg(long)]
        dry_run: bool,
    },

    /// Fetches the most recent snapshot from a trusted, pre-defined location.
    Fetch {
        /// Directory to which the snapshot should be downloaded. If not
        /// provided, it will be saved in default Forest data location.
        #[arg(short, long)]
        snapshot_dir: Option<PathBuf>,
        /// Maximum number of times to retry the fetch
        #[arg(short, long, default_value = "3")]
        max_retries: usize,
        /// Duration to wait between the retries in seconds
        #[arg(short, long, default_value = "60", value_parser = parse_duration)]
        delay: Duration,
    },

    /// Shows default snapshot dir
    Dir,

    /// List local snapshots
    List {
        /// Directory to which the snapshots are downloaded. If not provided, it
        /// will be the default Forest data location.
        #[arg(short, long)]
        snapshot_dir: Option<PathBuf>,
    },

    /// Remove all known snapshots except the latest
    Prune {
        /// Directory to which the snapshots are downloaded. If not provided, it
        /// will be the default Forest data location.
        // TODO(aatifsyed): lift this parameter up rather than repeating it everywhere
        #[arg(short, long)]
        snapshot_dir: Option<PathBuf>,
    },

    /// Clean all local snapshots, use with care.
    Clean {
        /// Directory to which the snapshots are downloaded. If not provided, it
        /// will be the default Forest data location.
        #[arg(short, long)]
        snapshot_dir: Option<PathBuf>,
    },

    /// Validates the snapshot.
    Validate {
        /// Number of block headers to validate from the tip
        #[arg(long, default_value = "2000")]
        recent_stateroots: i64,
        /// Path to snapshot file
        snapshot: PathBuf,
        /// Force validation and answers yes to all prompts.
        #[arg(long)]
        force: bool,
    },
}

impl SnapshotCommands {
    pub async fn run(&self, config: Config) -> anyhow::Result<()> {
        match self {
            Self::Export {
                output_path,
                skip_checksum,
                dry_run,
            } => {
                let chain_head = match chain_head(&config.client.rpc_token).await {
                    Ok(head) => head.0,
                    Err(_) => cli_error_and_die("Could not get network head", 1),
                };

                let epoch = chain_head.epoch();

                let chain_name = chain_get_name((), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;

                let output_path = match output_path.is_dir() {
                    true => output_path.join(
                        Utc::now()
                            .format(&format!(
                                "forest_snapshot_{chain_name}_%Y-%m-%d_height_{epoch}.car.zst"
                            ))
                            .to_string(),
                    ),
                    false => output_path.clone(),
                };

                let params = ChainExportParams {
                    epoch,
                    recent_roots: config.chain.recent_state_roots,
                    output_path,
                    tipset_keys: TipsetKeysJson(chain_head.key().clone()),
                    compressed: true,
                    skip_checksum: *skip_checksum,
                    dry_run: *dry_run,
                };

                let bar = Arc::new(tokio::sync::Mutex::new({
                    let bar = ProgressBar::new(0);
                    bar.message("Exporting snapshot | blocks ");
                    bar
                }));
                tokio::spawn({
                    let bar = bar.clone();
                    async move {
                        let mut interval =
                            tokio::time::interval(tokio::time::Duration::from_secs(1));
                        loop {
                            interval.tick().await;
                            if let Ok((progress, total)) =
                                get_progress((GetProgressType::SnapshotExport,), &None).await
                            {
                                let bar = bar.lock().await;
                                if bar.is_finish() {
                                    break;
                                }
                                bar.set_total(total);
                                bar.set(progress);
                            }
                        }
                    }
                });

                let out = chain_export(params, &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;

                bar.lock().await.finish_println(&format!(
                    "Export completed. Snapshot located at {}",
                    out.display()
                ));
                Ok(())
            }
            Self::Fetch {
                snapshot_dir,
                max_retries,
                delay,
            } => {
                let client = reqwest::Client::new();
                let snapshot_dir = override_or_default(snapshot_dir, &config)?;
                let progress_bar_style = indicatif::ProgressStyle::with_template(
                    "{msg:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes}",
                )
                .expect("invalid progress template")
                .progress_chars("#>-");
                let progress_bar = indicatif::ProgressBar::new(0)
                    .with_message("downloading snapshot")
                    .with_style(progress_bar_style);
                let (slug, stable_url) = match config.chain.name.to_lowercase().as_str() {
                    "mainnet" => ("mainnet", config.snapshot_fetch.filecoin.mainnet_compressed),
                    "calibnet" | "calibrationnet" => (
                        "calibnet",
                        config.snapshot_fetch.filecoin.calibnet_compressed,
                    ),
                    name => bail!("unsupported chain name: {name}"),
                };
                match retry(
                    RetryArgs {
                        timeout: None,
                        max_retries: Some(*max_retries),
                        delay: Some(*delay),
                    },
                    || {
                        forest_cli_shared::snapshot::fetch(
                            snapshot_dir.as_canonical_path(),
                            slug,
                            &client,
                            stable_url.clone(),
                            &progress_bar,
                        )
                    },
                )
                .await
                {
                    Ok(out) => {
                        println!("Snapshot successfully downloaded at {}", out.display());
                        Ok(())
                    }
                    Err(e) => cli_error_and_die(format!("Failed fetching the snapshot: {e}"), 1),
                }
            }
            // TODO(aatifsyed): this is a confusing name - `dir` and `list` are synonymous in some
            // contexts
            Self::Dir => {
                let dir = default_snapshot_dir(&config);
                println!("{}", dir.display());
                Ok(())
            }
            Self::List { snapshot_dir } => {
                let snapshot_dir = override_or_default(snapshot_dir, &config)?;
                let snapshots =
                    forest_cli_shared::snapshot::list(snapshot_dir.as_canonical_path())?;
                if snapshots.is_empty() {
                    eprintln!("no snapshots")
                } else {
                    for snapshot in snapshots {
                        println!("snapshot:");
                        println!("\tpath: {}", snapshot.path.display());
                        println!("\theight: {}", snapshot.metadata.height);
                        println!("\tdatetime: {}", snapshot.metadata.datetime);
                        println!("\tslug: {}", snapshot.slug);
                    }
                }
                Ok(())
            }
            Self::Prune { snapshot_dir } => {
                let snapshot_dir = override_or_default(snapshot_dir, &config)?;
                let snapshots =
                    forest_cli_shared::snapshot::list(snapshot_dir.as_canonical_path())?;
                let Some( oldest) = snapshots.iter().max_by_key(|snap| snap.metadata.datetime) else {
                    eprintln!("no snapshots");
                    return Ok(());
                };
                for snapshot in snapshots.iter().filter(|it| *it != oldest) {
                    if let Err(e) = fs::remove_file(&snapshot.path) {
                        error!("Error removing snapshot: {e}");
                    }
                }
                Ok(())
            }
            Self::Clean { snapshot_dir } => {
                let snapshot_dir = override_or_default(snapshot_dir, &config)?;
                forest_cli_shared::snapshot::clean(snapshot_dir.as_canonical_path())?;
                Ok(())
            }
            Self::Validate {
                recent_stateroots,
                snapshot,
                force,
            } => validate(&config, recent_stateroots, snapshot, *force).await,
        }
    }
}

/// TODO(aatifsyed): this makes a blocking syscall, but we're the only task
/// running on the executor, so probably not a big deal for now
fn override_or_default(
    user_override: &Option<PathBuf>,
    config: &Config,
) -> anyhow::Result<CanonicalPathBuf> {
    let snapshot_dir = user_override
        .clone()
        .unwrap_or_else(|| default_snapshot_dir(config));
    let snapshot_dir =
        CanonicalPathBuf::new(snapshot_dir).context("couldn't canonicalize snapshot dir")?;
    Ok(snapshot_dir)
}

async fn validate(
    config: &Config,
    recent_stateroots: &i64,
    snapshot: &PathBuf,
    force: bool,
) -> anyhow::Result<()> {
    let confirm = force
        || atty::is(atty::Stream::Stdin)
            && Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(format!(
                    "This will result in using approximately {} MB of data. Proceed?",
                    std::fs::metadata(snapshot)?.len() / (1024 * 1024)
                ))
                .default(false)
                .interact()
                .unwrap_or_default();

    if confirm {
        let tmp_chain_data_path = TempDir::new()?;
        let db_path = db_root(
            tmp_chain_data_path
                .path()
                .join(config.chain.network.to_string())
                .as_path(),
        );
        let db = open_proxy_db(db_path, config.db_config().clone())?;

        let genesis = read_genesis_header(
            config.client.genesis_file.as_ref(),
            config.chain.genesis_bytes(),
            &db,
        )
        .await?;

        let chain_store = Arc::new(ChainStore::new(
            db,
            config.chain.clone(),
            &genesis,
            tmp_chain_data_path.path(),
        )?);

        let (cids, _n_records) = {
            let reader = get_fetch_progress_from_file(&snapshot).await?;
            forest_load_car(chain_store.blockstore().clone(), reader).await?
        };

        let ts = chain_store.tipset_from_keys(&TipsetKeys::new(cids))?;

        validate_links_and_genesis_traversal(
            &chain_store,
            ts,
            chain_store.blockstore(),
            *recent_stateroots,
            &Tipset::from(genesis),
            &config.chain.network,
        )
        .await?;
    }

    Ok(())
}

async fn validate_links_and_genesis_traversal<DB>(
    chain_store: &ChainStore<DB>,
    ts: Arc<Tipset>,
    db: &DB,
    recent_stateroots: ChainEpoch,
    genesis_tipset: &Tipset,
    network: &NetworkChain,
) -> anyhow::Result<()>
where
    DB: fvm_ipld_blockstore::Blockstore + Send + Sync,
{
    let mut seen = CidHashSet::default();
    let upto = ts.epoch() - recent_stateroots;

    let mut tsk = ts.parents().clone();

    let total_size = ts.epoch();
    let pb = forest_utils::io::ProgressBar::new(total_size as u64);
    pb.message("Validating tipsets: ");
    pb.set_max_refresh_rate(Some(std::time::Duration::from_millis(500)));

    // Security: Recursive snapshots are difficult to create but not impossible.
    // This limits the amount of recursion we do.
    let mut prev_epoch = ts.epoch();
    loop {
        // if we reach 0 here, it means parent traversal didn't end up reaching genesis
        // properly, bail with error.
        if prev_epoch <= 0 {
            bail!("Broken invariant: no genesis tipset in snapshot.");
        }

        let tipset = chain_store.tipset_from_keys(&tsk)?;
        let height = tipset.epoch();
        // if parent tipset epoch is smaller than child, bail with error.
        if height >= prev_epoch {
            bail!("Broken tipset invariant: parent epoch larger than current epoch at: {height}");
        }
        // genesis is reachable, break with success
        if height == 0 {
            if tipset.as_ref() != genesis_tipset {
                bail!("Invalid genesis tipset. Snapshot isn't valid for {network}. It may be valid for another network.");
            }

            break;
        }
        // check for ipld links backwards till `upto`
        if height > upto {
            let mut assert_cid_exists = |cid: Cid| async move {
                let data = db.get(&cid)?;
                data.ok_or_else(|| anyhow::anyhow!("Broken IPLD link at epoch: {height}"))
            };

            for h in tipset.blocks() {
                recurse_links_hash(&mut seen, *h.state_root(), &mut assert_cid_exists, &|_| ())
                    .await?;
                recurse_links_hash(&mut seen, *h.messages(), &mut assert_cid_exists, &|_| ())
                    .await?;
            }
        }

        tsk = tipset.parents().clone();
        prev_epoch = tipset.epoch();
        pb.set((ts.epoch() - tipset.epoch()) as u64);
    }

    drop(pb);

    println!("Snapshot is valid");

    Ok(())
}
