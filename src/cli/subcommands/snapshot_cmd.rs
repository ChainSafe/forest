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
use crate::daemon::bundle::load_bundles;
use crate::fil_cns::composition as cns;
use crate::genesis::read_genesis_header;
use crate::ipld::{recurse_links_hash, CidHashSet};
use crate::networks::{calibnet, mainnet, ChainConfig, NetworkChain};
use crate::rpc_api::{chain_api::ChainExportParams, progress_api::GetProgressType};
use crate::rpc_client::{chain_ops::*, progress_ops::get_progress};
use crate::state_manager::StateManager;
use crate::utils::{io::ProgressBar, proofs_api::paramfetch::ensure_params_downloaded};
use anyhow::{bail, Context, Result};
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
        /// Tipset to start the export from, default is the chain head
        #[arg(short, long)]
        tipset: Option<i64>,
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
        /// Number of recent epochs to scan for broken links
        #[arg(long, default_value_t = 2000)]
        check_links: u32,
        /// Assert the snapshot belongs to this network. If left blank, the
        /// network will be inferred before executing messages.
        #[arg(long)]
        check_network: Option<crate::networks::NetworkChain>,
        /// Number of recent epochs to scan for bad messages/transactions
        #[arg(long, default_value_t = 60)]
        check_stateroots: u32,
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
    pub async fn run(self, config: Config) -> Result<()> {
        match self {
            Self::Export {
                output_path,
                skip_checksum,
                dry_run,
                tipset,
            } => {
                let chain_head = match chain_head(&config.client.rpc_token).await {
                    Ok(head) => head.0,
                    Err(_) => cli_error_and_die("Could not get network head", 1),
                };

                let epoch = tipset.unwrap_or(chain_head.epoch());

                let chain_name = chain_get_name((), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;

                let output_path = match output_path.is_dir() {
                    true => output_path.join(snapshot::filename(
                        TrustedVendor::Forest,
                        chain_name,
                        Utc::now().date_naive(),
                        epoch,
                    )),
                    false => output_path.clone(),
                };

                let params = ChainExportParams {
                    epoch,
                    recent_roots: config.chain.recent_state_roots,
                    output_path,
                    tipset_keys: TipsetKeysJson(chain_head.key().clone()),
                    skip_checksum,
                    dry_run,
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
                println!("\n");
                Ok(())
            }
            Self::Fetch { directory, vendor } => {
                match snapshot::fetch(&directory, &config.chain.network, vendor).await {
                    Ok(out) => {
                        println!("{}", out.display());
                        Ok(())
                    }
                    Err(e) => cli_error_and_die(format!("Failed fetching the snapshot: {e}"), 1),
                }
            }
            Self::Validate {
                check_links,
                check_network,
                check_stateroots,
                snapshot,
            } => {
                // this is all blocking...
                use std::fs::File;
                match CompressedCarV1BackedBlockstore::new(BufReader::new(File::open(&snapshot)?)) {
                    Ok(store) => {
                        validate_with_blockstore(
                            store.roots(),
                            Arc::new(store),
                            check_links,
                            check_network,
                            check_stateroots,
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
                        let store = UncompressedCarV1BackedBlockstore::new(File::open(&snapshot)?)?;
                        validate_with_blockstore(
                            store.roots(),
                            Arc::new(store),
                            check_links,
                            check_network,
                            check_stateroots,
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
                        File::open(&source).await?,
                    )),
                    File::create(&destination).await?,
                    frame_size,
                    compression_level,
                )
                .await
                {
                    Ok(_num_frames) => Ok(()),
                    Err(error) => {
                        info!(%error, "file may be uncompressed, retrying as a plain CAR...");
                        zstd_compress_varint_manyframe(
                            File::open(&source).await?,
                            File::create(&destination).await?,
                            frame_size,
                            compression_level,
                        )
                        .await?;
                        Ok(())
                    }
                }
            }
        }
    }
}

// Check the validity of a snapshot by looking at IPLD links, the genesis block,
// and message output. More checks may be added in the future.
//
// If the snapshot is valid, the output should look like this:
//     Checking IPLD integrity:       ✅ verified!
//     Identifying genesis block:     ✅ found!
//     Verifying network identity:    ✅ verified!
//     Running tipset transactions:   ✅ verified!
//   Snapshot is valid
//
// If we receive a mainnet snapshot but expect a calibnet snapshot, the output
// should look like this:
//     Checking IPLD integrity:       ✅ verified!
//     Identifying genesis block:     ✅ found!
//     Verifying network identity:    ❌ wrong!
//   Error: Expected mainnet but found calibnet
async fn validate_with_blockstore<BlockstoreT>(
    roots: Vec<Cid>,
    store: Arc<BlockstoreT>,
    check_links: u32,
    check_network: Option<NetworkChain>,
    check_stateroots: u32,
) -> Result<()>
where
    BlockstoreT: Blockstore + Send + Sync + 'static,
{
    let tipset_key = TipsetKeys::new(roots);
    let store_clone = Arc::clone(&store);
    let ts = Tipset::load(&store_clone, &tipset_key)?.context("missing root tipset")?;

    if check_links != 0 {
        validate_ipld_links(ts.clone(), &store, check_links).await?;
    }

    if let Some(expected_network) = &check_network {
        let actual_network = query_network(ts.clone(), &store)?;
        // Somewhat silly use of a spinner but this makes the checks line up nicely.
        let pb = validation_spinner("Verifying network identity:");
        if expected_network != &actual_network {
            pb.finish_with_message("❌ wrong!");
            bail!("Expected {} but found {}", expected_network, actual_network);
        } else {
            pb.finish_with_message("✅ verified!");
        }
    }

    if check_stateroots != 0 {
        let network = check_network
            .map(anyhow::Ok)
            .unwrap_or_else(|| query_network(ts.clone(), &store))?;
        validate_stateroots(ts, &store, network, check_stateroots).await?;
    }

    println!("Snapshot is valid");
    Ok(())
}

// The Filecoin block chain is a DAG of Ipld nodes. The complete graph isn't
// required to sync to the network and snapshot files usually disgard data after
// 2000 epochs. Validity can be verified by ensuring there are no bad IPLD or
// broken links in the N most recent epochs.
async fn validate_ipld_links<DB>(ts: Tipset, db: &DB, epochs: u32) -> Result<()>
where
    DB: Blockstore + Send + Sync,
{
    let epoch_limit = ts.epoch() - epochs as i64;
    let mut seen = CidHashSet::default();

    let pb = validation_spinner("Checking IPLD integrity:").with_finish(
        indicatif::ProgressFinish::AbandonWithMessage("❌ Invalid IPLD data!".into()),
    );

    for tipset in ts
        .chain(db)
        .take_while(|tipset| tipset.epoch() > epoch_limit)
    {
        let height = tipset.epoch();
        pb.set_message(format!("{} remaining epochs", height - epoch_limit));

        let mut assert_cid_exists = |cid: Cid| async move {
            let data = db.get(&cid)?;
            data.ok_or_else(|| anyhow::anyhow!("Broken IPLD link at epoch: {height}"))
        };

        for h in tipset.blocks() {
            recurse_links_hash(&mut seen, *h.state_root(), &mut assert_cid_exists, &|_| ()).await?;
            recurse_links_hash(&mut seen, *h.messages(), &mut assert_cid_exists, &|_| ()).await?;
        }
    }

    pb.finish_with_message("✅ verified!");
    Ok(())
}

// The genesis block determines the network identity (e.g., mainnet or
// calibnet). Scanning through the entire blockchain can be time-consuming, so
// Forest keeps a list of known tipsets for each network. Finding a known tipset
// short-circuits the search for the genesis block. If no genesis block can be
// found or if the genesis block is unrecognizable, an error is returned.
fn query_network<DB>(ts: Tipset, db: &DB) -> Result<NetworkChain>
where
    DB: Blockstore + Send + Sync + Clone + 'static,
{
    let pb = validation_spinner("Identifying genesis block:").with_finish(
        indicatif::ProgressFinish::AbandonWithMessage("✅ found!".into()),
    );

    fn match_genesis_block(block_cid: Cid) -> Result<NetworkChain> {
        if block_cid.to_string() == calibnet::GENESIS_CID {
            Ok(NetworkChain::Calibnet)
        } else if block_cid.to_string() == mainnet::GENESIS_CID {
            Ok(NetworkChain::Mainnet)
        } else {
            bail!("Unrecognizable genesis block");
        }
    }

    if let Ok(genesis_block) = ts.genesis(db) {
        return match_genesis_block(*genesis_block.cid());
    }

    pb.finish_with_message("❌ No valid genesis block!");
    bail!("Snapshot does not contain a genesis block")
}

// Each tipset in the blockchain contains a set of messages. A message is a
// transaction that manipulates a persistent state-tree. The hashes of these
// state-trees are stored in the tipsets and can be used to verify if the
// messages were correctly executed.
// Note: Messages may access state-trees 900 epochs in the past. So, if a
// snapshot has state-trees for 2000 epochs, one can only validate the messages
// for the last 1100 epochs.
async fn validate_stateroots<DB>(
    ts: Tipset,
    db: &DB,
    network: NetworkChain,
    epochs: u32,
) -> Result<()>
where
    DB: Blockstore + Send + Sync + Clone + 'static,
{
    let chain_config = Arc::new(ChainConfig::from_chain(&network));
    let genesis = read_genesis_header(None, chain_config.genesis_bytes(), db).await?;

    let chain_data_root = TempDir::new()?;
    let chain_store = Arc::new(ChainStore::new(
        Arc::new(db.clone()),
        Arc::clone(&chain_config),
        &genesis,
        chain_data_root.path(),
    )?);

    let pb = validation_spinner("Running tipset transactions:").with_finish(
        indicatif::ProgressFinish::AbandonWithMessage(
            "❌ Transaction result differs from Lotus!".into(),
        ),
    );

    let last_epoch = ts.epoch() - epochs as i64;

    // Bundles are required when doing state migrations. Download any bundles
    // that may be necessary after `last_epoch`.
    load_bundles(
        last_epoch,
        &Config {
            chain: chain_config.clone(),
            ..Default::default()
        },
        Arc::new(db.clone()),
    )
    .await?;

    // Set proof parameter data dir
    if cns::FETCH_PARAMS {
        crate::utils::proofs_api::paramfetch::set_proofs_parameter_cache_dir_env(
            &Config::default().client.data_dir,
        );
    }
    // Initialize StateManager
    let state_manager = Arc::new(StateManager::new(chain_store, Arc::clone(&chain_config))?);
    ensure_params_downloaded().await?;

    // Prepare tipsets for validation
    let tipsets = ts
        .chain(db)
        .map(|ts| Arc::clone(&Arc::new(ts)))
        .take_while(|tipset| tipset.epoch() >= last_epoch)
        .inspect(|tipset| {
            pb.set_message(format!("epoch queue: {}", tipset.epoch() - last_epoch));
        });

    // ProgressBar::wrap_iter believes the progress has been abandoned once the
    // iterator is consumed.
    state_manager.validate_tipsets(tipsets)?;

    pb.finish_with_message("✅ verified!");
    drop(pb);
    Ok(())
}

fn validation_spinner(prefix: &'static str) -> indicatif::ProgressBar {
    let pb = indicatif::ProgressBar::new_spinner()
        .with_style(
            indicatif::ProgressStyle::with_template("{spinner} {prefix:<30} {msg}")
                .expect("indicatif template must be valid"),
        )
        .with_prefix(prefix);
    pb.enable_steady_tick(std::time::Duration::from_secs_f32(0.1));
    pb
}
