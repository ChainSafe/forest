// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{path::PathBuf, sync::Arc};

use crate::blocks::{tipset_keys_json::TipsetKeysJson, Tipset, TipsetKeys};
use crate::chain::ChainStore;
use crate::cli_shared::snapshot::{self, TrustedVendor};
use crate::db::db_engine::{db_root, open_proxy_db};
use crate::genesis::{forest_load_car, read_genesis_header};
use crate::ipld::{recurse_links_hash, CidHashSet};
use crate::networks::NetworkChain;
use crate::rpc_api::{chain_api::ChainExportParams, progress_api::GetProgressType};
use crate::rpc_client::{chain_ops::*, progress_ops::get_progress};
use crate::shim::clock::ChainEpoch;
use crate::utils::io::ProgressBar;
use anyhow::bail;
use chrono::Utc;
use clap::Subcommand;
use dialoguer::{theme::ColorfulTheme, Confirm};
use tempfile::TempDir;
use tokio_util::compat::TokioAsyncReadCompatExt;

use super::*;

use crate::cli::subcommands::{cli_error_and_die, handle_rpc_err};

#[derive(Debug, Subcommand)]
pub enum SnapshotCommands {
    /// Export a snapshot of the chain to `<output_path>`
    Export {
        /// Snapshot output filename or directory. Defaults to
        /// `./forest_snapshot_{chain}_{year}-{month}-{day}_height_{epoch}.car.zst`.
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
        #[arg(short, long, default_value = ".")]
        directory: PathBuf,
        /// Vendor to fetch the snapshot from
        #[arg(short, long, value_enum, default_value_t = snapshot::TrustedVendor::default())]
        vendor: snapshot::TrustedVendor,
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
                    true => output_path.join(snapshot::filename(
                        TrustedVendor::Forest,
                        chain_name,
                        Utc::now().date_naive(),
                        chain_head.epoch(),
                    )),
                    false => output_path.clone(),
                };

                let params = ChainExportParams {
                    epoch,
                    recent_roots: config.chain.recent_state_roots,
                    output_path,
                    tipset_keys: TipsetKeysJson(chain_head.key().clone()),
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
            Self::Fetch { directory, vendor } => {
                match snapshot::fetch(directory, &config.chain.network, *vendor).await {
                    Ok(out) => {
                        println!("{}", out.display());
                        Ok(())
                    }
                    Err(e) => cli_error_and_die(format!("Failed fetching the snapshot: {e}"), 1),
                }
            }
            Self::Validate {
                recent_stateroots,
                snapshot,
                force,
            } => validate(&config, recent_stateroots, snapshot, *force).await,
        }
    }
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
            let reader =
                crate::utils::net::reader(snapshot.as_path().display().to_string().as_str())
                    .await?;
            forest_load_car(chain_store.blockstore().clone(), reader.compat()).await?
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
    let pb = crate::utils::io::ProgressBar::new(total_size as u64);
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
