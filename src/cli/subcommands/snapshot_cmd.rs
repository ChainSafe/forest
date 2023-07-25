// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::blocks::{tipset_keys_json::TipsetKeysJson, Tipset, TipsetKeys};
use crate::car_backed_blockstore::{
    self, CompressedCarV1BackedBlockstore, MaxFrameSizeExceeded, UncompressedCarV1BackedBlockstore,
};
use crate::chain::index::ChainIndex;
use crate::cli::subcommands::{cli_error_and_die, handle_rpc_err};
use crate::cli_shared::snapshot::{self, TrustedVendor};
use crate::daemon::bundle::load_bundles;
use crate::fil_cns::composition as cns;
use crate::ipld::{recurse_links_hash, CidHashSet};
use crate::networks::{calibnet, mainnet, ChainConfig, NetworkChain};
use crate::rpc_api::chain_api::ChainExportParams;
use crate::rpc_client::chain_ops::*;
use crate::shim::machine::MultiEngine;
use crate::utils::proofs_api::paramfetch::ensure_params_downloaded;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use clap::Subcommand;
use fvm_ipld_blockstore::Blockstore;
use human_repr::HumanCount;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::io::AsyncWriteExt;
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

                let output_dir = output_path.parent().context("invalid output path")?;
                let temp_path = NamedTempFile::new_in(output_dir)?.into_temp_path();

                let params = ChainExportParams {
                    epoch,
                    recent_roots: config.chain.recent_state_roots,
                    output_path: temp_path.to_path_buf(),
                    tipset_keys: TipsetKeysJson(chain_head.key().clone()),
                    skip_checksum,
                    dry_run,
                };

                let handle = tokio::spawn({
                    let tmp_file = temp_path.to_owned();
                    let output_path = output_path.clone();
                    async move {
                        let mut interval =
                            tokio::time::interval(tokio::time::Duration::from_secs_f32(0.25));
                        println!("Getting ready to export...");
                        loop {
                            interval.tick().await;
                            let snapshot_size = std::fs::metadata(&tmp_file)
                                .map(|meta| meta.len())
                                .unwrap_or(0);
                            print!(
                                "{}{}",
                                anes::MoveCursorToPreviousLine(1),
                                anes::ClearLine::All
                            );
                            println!(
                                "{}: {}",
                                &output_path.to_string_lossy(),
                                snapshot_size.human_count_bytes()
                            );
                            let _ = std::io::stdout().flush();
                        }
                    }
                });

                let hash_result = chain_export(params, &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;

                handle.abort();
                let _ = handle.await;

                if let Some(hash) = hash_result {
                    save_checksum(&output_path, hash).await?;
                }
                temp_path.persist(output_path)?;

                println!("Export completed.");
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
                    <usize as integer_encoding::VarInt>::decode_var(&[0xFD, 0x2F, 0xB5, 0x28])
                        .unwrap()
                        .1,
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

/// Prints hex-encoded representation of SHA-256 checksum and saves it to a file
/// with the same name but with a `.sha256sum` extension.
async fn save_checksum(source: &Path, encoded_hash: String) -> Result<()> {
    let checksum_file_content = format!(
        "{encoded_hash} {}\n",
        source
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .context("Failed to retrieve file name while saving checksum")?
    );

    let checksum_path = PathBuf::from(source).with_extension("sha256sum");

    let mut checksum_file = tokio::fs::File::create(&checksum_path).await?;
    checksum_file
        .write_all(checksum_file_content.as_bytes())
        .await?;
    checksum_file.flush().await?;
    Ok(())
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
        if block_cid == *calibnet::GENESIS_CID {
            Ok(NetworkChain::Calibnet)
        } else if block_cid == *mainnet::GENESIS_CID {
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
    let genesis = ts.genesis(db)?;

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
        &db,
    )
    .await?;

    // Set proof parameter data dir and make sure the proofs are available
    if cns::FETCH_PARAMS {
        crate::utils::proofs_api::paramfetch::set_proofs_parameter_cache_dir_env(
            &Config::default().client.data_dir,
        );
    }
    ensure_params_downloaded().await?;

    let chain_index = Arc::new(ChainIndex::new(Arc::new(db.clone())));

    // Prepare tipsets for validation
    let tipsets = chain_index
        .chain(Arc::new(ts))
        .take_while(|tipset| tipset.epoch() >= last_epoch)
        .inspect(|tipset| {
            pb.set_message(format!("epoch queue: {}", tipset.epoch() - last_epoch));
        });

    let beacon = Arc::new(chain_config.get_beacon_schedule(genesis.timestamp()));

    // ProgressBar::wrap_iter believes the progress has been abandoned once the
    // iterator is consumed.
    crate::state_manager::validate_tipsets(
        genesis.timestamp(),
        chain_index.clone(),
        chain_config,
        beacon,
        &MultiEngine::default(),
        tipsets,
    )?;

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
