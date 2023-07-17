// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::blocks::{tipset_keys_json::TipsetKeysJson, Tipset, TipsetKeys};
use crate::car_backed_blockstore::{
    self, CompressedCarV1BackedBlockstore, MaxFrameSizeExceeded, UncompressedCarV1BackedBlockstore,
};
use crate::chain::ChainStore;
use crate::cli::subcommands::{cli_error_and_die, handle_rpc_err};
use crate::cli_shared::snapshot::{self, TrustedVendor};
use crate::fil_cns::composition as cns;
use crate::genesis::read_genesis_header;
use crate::ipld::{recurse_links_hash, CidHashSet};
use crate::networks::NetworkChain;
use crate::rpc_api::{chain_api::ChainExportParams, progress_api::GetProgressType};
use crate::rpc_client::{chain_ops::*, progress_ops::get_progress};
use crate::shim::clock::ChainEpoch;
use crate::state_manager::StateManager;
use crate::utils::{io::ProgressBar, proofs_api::paramfetch::ensure_params_downloaded};
use anyhow::{bail, Context};
use chrono::Utc;
use clap::Subcommand;
use fvm_ipld_blockstore::Blockstore;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use tracing::info;

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
        /// Validate already computed tipsets at given EPOCH,
        /// use a negative value -N to validate the last N EPOCH(s) starting at HEAD.
        #[arg(long)]
        validate_tipsets: Option<i64>,
        /// Path to a snapshot CAR, which may be zstd compressed
        snapshot: PathBuf,
    },
    /// Make this snapshot suitable for use as a compressed car-backed blockstore.
    Compress {
        /// CAR file. May be a zstd-compressed
        source: PathBuf,
        destination: PathBuf,
        #[arg(hide = true, long, default_value_t = 3)]
        compression_level: u16,
        /// End zstd frames after they exceed this length
        #[arg(hide = true, long, default_value_t = 8000usize.next_power_of_two())]
        frame_size: usize,
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
                validate_tipsets,
                snapshot,
            } => {
                // this is all blocking...
                use std::fs::File;
                match CompressedCarV1BackedBlockstore::new(BufReader::new(File::open(snapshot)?)) {
                    Ok(store) => {
                        validate_with_blockstore(
                            &config,
                            store.roots(),
                            Arc::new(store),
                            recent_stateroots,
                            *validate_tipsets,
                        )
                        .await
                    }
                    Err(error)
                        if error.kind() == std::io::ErrorKind::Other
                            && error.get_ref().is_some_and(|inner| {
                                inner.downcast_ref::<MaxFrameSizeExceeded>().is_some()
                            }) =>
                    {
                        bail!("The provided compressed car file cannot be used as a blockstore. Prepare it using `forest snapshot compress ...`")
                    }
                    Err(error) => {
                        info!(%error, "file may be uncompressed, retrying as a plain CAR...");
                        let store = UncompressedCarV1BackedBlockstore::new(File::open(snapshot)?)?;
                        validate_with_blockstore(
                            &config,
                            store.roots(),
                            Arc::new(store),
                            recent_stateroots,
                            *validate_tipsets,
                        )
                        .await
                    }
                }
            }
            Self::Compress {
                source,
                destination,
                compression_level,
                frame_size,
            } => {
                // We've got a binary blob, and we're not exactly sure if it's compressed, and we can't just peek the header:
                // For example, the zstsd magic bytes are a valid varint frame prefix:
                assert_eq!(
                    unsigned_varint::io::read_usize(&[0xFD, 0x2F, 0xB5, 0x28][..]).unwrap(),
                    6141,
                );
                // so the best thing to do is to just try compressed and then uncompressed.
                use car_backed_blockstore::zstd_compress_varint_manyframe;
                use tokio::fs::File;
                match zstd_compress_varint_manyframe(
                    async_compression::tokio::bufread::ZstdDecoder::new(tokio::io::BufReader::new(
                        File::open(source).await?,
                    )),
                    File::create(destination).await?,
                    *frame_size,
                    *compression_level,
                )
                .await
                {
                    Ok(_num_frames) => Ok(()),
                    Err(error) => {
                        info!(%error, "file may be uncompressed, retrying as a plain CAR...");
                        zstd_compress_varint_manyframe(
                            File::open(source).await?,
                            File::create(destination).await?,
                            *frame_size,
                            *compression_level,
                        )
                        .await?;
                        Ok(())
                    }
                }
            }
        }
    }
}

async fn validate_with_blockstore<BlockstoreT>(
    config: &Config,
    roots: Vec<Cid>,
    store: Arc<BlockstoreT>,
    recent_stateroots: &i64,
    validate_tipsets: Option<i64>,
) -> anyhow::Result<()>
where
    BlockstoreT: Blockstore + Send + Sync + 'static,
{
    let genesis = read_genesis_header(
        config.client.genesis_file.as_ref(),
        config.chain.genesis_bytes(),
        &store,
    )
    .await?;
    let chain_data_root = TempDir::new()?;
    let chain_store = Arc::new(ChainStore::new(
        Arc::clone(&store),
        config.chain.clone(),
        &genesis,
        chain_data_root.path(),
    )?);

    let ts = Tipset::load(&store, &TipsetKeys::new(roots))?.context("missing root tipset")?;

    validate_links_and_genesis_traversal(
        &chain_store,
        &ts,
        chain_store.blockstore(),
        *recent_stateroots,
        &Tipset::from(genesis),
        &config.chain.network,
    )
    .await?;

    if let Some(validate_from) = validate_tipsets {
        let last_epoch = match validate_from.is_negative() {
            true => ts.epoch() + validate_from,
            false => validate_from,
        };
        // Set proof parameter data dir
        if cns::FETCH_PARAMS {
            crate::utils::proofs_api::paramfetch::set_proofs_parameter_cache_dir_env(
                &config.client.data_dir,
            );
        }
        // Initialize StateManager
        let state_manager = Arc::new(StateManager::new(chain_store, Arc::clone(&config.chain))?);
        ensure_params_downloaded().await?;
        // Prepare tipset stream to validate
        let tipsets = ts
            .chain(&store)
            .map(|ts| Arc::clone(&Arc::new(ts)))
            .take_while(|tipset| tipset.epoch() >= last_epoch);

        state_manager.validate_tipsets(tipsets)?
    }

    println!("Snapshot is valid");
    Ok(())
}

async fn validate_links_and_genesis_traversal<DB>(
    chain_store: &ChainStore<DB>,
    ts: &Tipset,
    db: &DB,
    recent_stateroots: ChainEpoch,
    genesis_tipset: &Tipset,
    network: &NetworkChain,
) -> anyhow::Result<()>
where
    DB: Blockstore + Send + Sync,
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

    Ok(())
}
