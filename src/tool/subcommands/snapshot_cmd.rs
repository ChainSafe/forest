// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::blocks::Tipset;
use crate::chain::index::{ChainIndex, ResolveNullTipset};
use crate::cli_shared::snapshot;
use crate::daemon::bundle::load_actor_bundles;
use crate::db::car::forest::DEFAULT_FOREST_CAR_FRAME_SIZE;
use crate::db::car::{AnyCar, ManyCar};
use crate::db::{MemoryDB, PersistentStore};
use crate::interpreter::{MessageCallbackCtx, VMTrace};
use crate::ipld::stream_chain;
use crate::networks::{ChainConfig, NetworkChain, butterflynet, calibnet, mainnet};
use crate::shim::address::CurrentNetwork;
use crate::shim::clock::ChainEpoch;
use crate::shim::fvm_shared_latest::address::Network;
use crate::shim::machine::GLOBAL_MULTI_ENGINE;
use crate::state_manager::{StateOutput, apply_block_messages};
use crate::utils::db::car_stream::CarStream;
use crate::utils::proofs_api::ensure_proof_params_downloaded;
use anyhow::{Context as _, bail};
use cid::Cid;
use clap::Subcommand;
use dialoguer::{Confirm, theme::ColorfulTheme};
use futures::TryStreamExt;
use fvm_ipld_blockstore::Blockstore;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Subcommand)]
pub enum SnapshotCommands {
    /// Fetches the most recent snapshot from a trusted, pre-defined location.
    Fetch {
        #[arg(short, long, default_value = ".")]
        directory: PathBuf,
        /// Network chain the snapshot will belong to
        #[arg(long, default_value_t = NetworkChain::Mainnet)]
        chain: NetworkChain,
        /// Vendor to fetch the snapshot from
        #[arg(short, long, value_enum, default_value_t = snapshot::TrustedVendor::default())]
        vendor: snapshot::TrustedVendor,
    },

    /// Validate the provided snapshots as a whole.
    ValidateDiffs {
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
        #[arg(required = true)]
        snapshot_files: Vec<PathBuf>,
    },

    /// Validate the snapshots individually.
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
        #[arg(required = true)]
        snapshot_files: Vec<PathBuf>,
        /// Fail at the first invalid snapshot
        #[arg(long)]
        fail_fast: bool,
    },

    /// Make this snapshot suitable for use as a compressed car-backed blockstore.
    Compress {
        /// Input CAR file, in `.car`, `.car.zst`, or `.forest.car.zst` format.
        source: PathBuf,
        /// Output file, will be in `.forest.car.zst` format.
        ///
        /// Will reuse the source name (with new extension) if pointed to a
        /// directory.
        #[arg(short, long, default_value = ".")]
        output_path: PathBuf,
        #[arg(long, default_value_t = 3)]
        compression_level: u16,
        /// End zstd frames after they exceed this length
        #[arg(long, default_value_t = DEFAULT_FOREST_CAR_FRAME_SIZE)]
        frame_size: usize,
        /// Overwrite output file without prompting.
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    /// Filecoin keeps track of "the state of the world", including:
    /// wallets and their balances;
    /// storage providers and their deals;
    /// etc...
    ///
    /// It does this by (essentially) hashing the state of the world.
    ///
    /// The world can change when new blocks are mined and transmitted.
    /// A block may contain a message to e.g transfer FIL between two parties.
    /// Blocks are ordered by "epoch", which can be thought of as a timestamp.
    ///
    /// Snapshots contain (among other things) these messages.
    ///
    /// The command calculates the state of the world at EPOCH-1, applies all
    /// the messages at EPOCH, and prints the resulting hash of the state of the world.
    ///
    /// If --json is supplied, details about each message execution will printed.
    #[command(about = "Compute the state hash at a given epoch")]
    ComputeState {
        /// Path to a snapshot CAR, which may be zstd compressed
        snapshot: PathBuf,
        /// Which epoch to compute the state transition for
        #[arg(long)]
        epoch: ChainEpoch,
        /// Generate JSON output
        #[arg(long)]
        json: bool,
    },
}

impl SnapshotCommands {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Fetch {
                directory,
                chain,
                vendor,
            } => match snapshot::fetch(&directory, &chain, vendor).await {
                Ok(out) => {
                    println!("{}", out.display());
                    Ok(())
                }
                Err(e) => cli_error_and_die(format!("Failed fetching the snapshot: {e}"), 1),
            },
            Self::ValidateDiffs {
                check_links,
                check_network,
                check_stateroots,
                snapshot_files,
            } => {
                let store = ManyCar::try_from(snapshot_files)?;
                validate_with_blockstore(
                    store.heaviest_tipset()?,
                    Arc::new(store),
                    check_links,
                    check_network,
                    check_stateroots,
                )
                .await
            }
            Self::Validate {
                check_links,
                check_network,
                check_stateroots,
                snapshot_files,
                fail_fast,
            } => {
                let mut has_fail = false;
                for file in snapshot_files {
                    println!("Validating {}", file.display());
                    let result = async {
                        let store = ManyCar::new(MemoryDB::default())
                            .with_read_only(AnyCar::try_from(file.as_path())?)?;
                        validate_with_blockstore(
                            store.heaviest_tipset()?,
                            Arc::new(store),
                            check_links,
                            check_network.clone(),
                            check_stateroots,
                        )
                        .await?;
                        Ok::<(), anyhow::Error>(())
                    }
                    .await;
                    if let Err(e) = result {
                        has_fail = true;
                        eprintln!("Error: {e:?}");
                        if fail_fast {
                            break;
                        }
                    }
                }
                if has_fail {
                    bail!("validate failed");
                };
                Ok(())
            }
            Self::Compress {
                source,
                output_path,
                compression_level,
                frame_size,
                force,
            } => {
                // If input is 'snapshot.car.zst' and output is '.', set the
                // destination to './snapshot.forest.car.zst'.
                let destination = match output_path.is_dir() {
                    true => {
                        let mut destination = output_path;
                        destination.push(source.clone());
                        while let Some(ext) = destination.extension() {
                            if !(ext == "zst" || ext == "car" || ext == "forest") {
                                break;
                            }
                            destination.set_extension("");
                        }
                        destination.with_extension("forest.car.zst")
                    }
                    false => output_path.clone(),
                };

                if !force && destination.exists() {
                    let have_permission = Confirm::with_theme(&ColorfulTheme::default())
                        .with_prompt(format!(
                            "{} will be overwritten. Continue?",
                            destination.to_string_lossy()
                        ))
                        .default(false)
                        .interact()
                        // e.g not a tty (or some other error), so haven't got permission.
                        .unwrap_or(false);
                    if !have_permission {
                        return Ok(());
                    }
                }

                println!("Generating forest.car.zst file: {:?}", &destination);

                let file = File::open(&source).await?;
                let pb = ProgressBar::new(file.metadata().await?.len()).with_style(
                    ProgressStyle::with_template("{bar} {percent}%, eta: {eta}")
                        .expect("infallible"),
                );
                let file = tokio::io::BufReader::new(pb.wrap_async_read(file));

                let mut block_stream = CarStream::new(file).await?;
                let roots = std::mem::replace(
                    &mut block_stream.header_v1.roots,
                    nunny::vec![Default::default()],
                );

                let mut dest = tokio::io::BufWriter::new(File::create(&destination).await?);

                let frames = crate::db::car::forest::Encoder::compress_stream(
                    frame_size,
                    compression_level,
                    block_stream.map_err(anyhow::Error::from),
                );
                crate::db::car::forest::Encoder::write(&mut dest, roots, frames).await?;
                dest.flush().await?;
                Ok(())
            }
            SnapshotCommands::ComputeState {
                snapshot,
                epoch,
                json,
            } => print_computed_state(snapshot, epoch, json),
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
    root: Tipset,
    store: Arc<BlockstoreT>,
    check_links: u32,
    check_network: Option<NetworkChain>,
    check_stateroots: u32,
) -> anyhow::Result<()>
where
    BlockstoreT: PersistentStore + Send + Sync + 'static,
{
    if check_links != 0 {
        validate_ipld_links(root.clone(), &store, check_links).await?;
    }

    if let Some(expected_network) = &check_network {
        let actual_network = query_network(&root, &store)?;
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
            .unwrap_or_else(|| query_network(&root, &store))?;
        validate_stateroots(root, &store, network, check_stateroots).await?;
    }

    println!("Snapshot is valid");
    Ok(())
}

// The Filecoin block chain is a DAG of Ipld nodes. The complete graph isn't
// required to sync to the network and snapshot files usually discard data after
// 2000 epochs. Validity can be verified by ensuring there are no bad IPLD or
// broken links in the N most recent epochs.
async fn validate_ipld_links<DB>(ts: Tipset, db: &DB, epochs: u32) -> anyhow::Result<()>
where
    DB: Blockstore + Send + Sync,
{
    let epoch_limit = ts.epoch() - epochs as i64;

    let pb = validation_spinner("Checking IPLD integrity:").with_finish(
        indicatif::ProgressFinish::AbandonWithMessage("❌ Invalid IPLD data!".into()),
    );

    let tipsets = ts.chain(db).inspect(|tipset| {
        let height = tipset.epoch();
        if height - epoch_limit >= 0 {
            pb.set_message(format!("{} remaining epochs (state)", height - epoch_limit));
        } else {
            pb.set_message(format!("{height} remaining epochs (spine)"));
        }
    });
    let mut stream = stream_chain(&db, tipsets, epoch_limit);
    while stream.try_next().await?.is_some() {}

    pb.finish_with_message("✅ verified!");
    Ok(())
}

// The genesis block determines the network identity (e.g., mainnet or
// calibnet). Scanning through the entire blockchain can be time-consuming, so
// Forest keeps a list of known tipsets for each network. Finding a known tipset
// short-circuits the search for the genesis block. If no genesis block can be
// found or if the genesis block is unrecognizable, an error is returned.
fn query_network(ts: &Tipset, db: &impl Blockstore) -> anyhow::Result<NetworkChain> {
    let pb = validation_spinner("Identifying genesis block:").with_finish(
        indicatif::ProgressFinish::AbandonWithMessage("✅ found!".into()),
    );

    fn match_genesis_block(block_cid: Cid) -> anyhow::Result<NetworkChain> {
        if block_cid == *calibnet::GENESIS_CID {
            Ok(NetworkChain::Calibnet)
        } else if block_cid == *mainnet::GENESIS_CID {
            Ok(NetworkChain::Mainnet)
        } else if block_cid == *butterflynet::GENESIS_CID {
            Ok(NetworkChain::Butterflynet)
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
    db: &Arc<DB>,
    network: NetworkChain,
    epochs: u32,
) -> anyhow::Result<()>
where
    DB: PersistentStore + Send + Sync + 'static,
{
    let chain_config = Arc::new(ChainConfig::from_chain(&network));
    let genesis = ts.genesis(db)?;

    let pb = validation_spinner("Running tipset transactions:").with_finish(
        indicatif::ProgressFinish::AbandonWithMessage(
            "❌ Transaction result differs from Lotus!".into(),
        ),
    );

    // Fix off-by-1 bug: prevent validating more epochs than available in the snapshot.
    // Without +1, specifying --check-stateroots=900 would validate 901 epochs,
    // causing out-of-bounds errors when the snapshot contains only 900 recent state roots.
    let last_epoch = ts.epoch() - epochs as i64 + 1;

    // Bundles are required when doing state migrations.
    load_actor_bundles(&db, &network).await?;

    // Set proof parameter data dir and make sure the proofs are available
    crate::utils::proofs_api::maybe_set_proofs_parameter_cache_dir_env(
        &Config::default().client.data_dir,
    );

    ensure_proof_params_downloaded().await?;

    let chain_index = Arc::new(ChainIndex::new(Arc::new(db.clone())));

    // Prepare tipsets for validation
    let tipsets = chain_index
        .chain(Arc::new(ts))
        .take_while(|tipset| tipset.epoch() >= last_epoch)
        .inspect(|tipset| {
            pb.set_message(format!("epoch queue: {}", tipset.epoch() - last_epoch));
        });

    let beacon = Arc::new(chain_config.get_beacon_schedule(genesis.timestamp));

    // ProgressBar::wrap_iter believes the progress has been abandoned once the
    // iterator is consumed.
    crate::state_manager::validate_tipsets(
        genesis.timestamp,
        chain_index.clone(),
        chain_config,
        beacon,
        &GLOBAL_MULTI_ENGINE,
        tipsets,
    )?;

    pb.finish_with_message("✅ verified!");
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

fn print_computed_state(snapshot: PathBuf, epoch: ChainEpoch, json: bool) -> anyhow::Result<()> {
    // Initialize Blockstore
    let store = Arc::new(AnyCar::try_from(snapshot.as_path())?);

    // Prepare call to apply_block_messages
    let ts = store.heaviest_tipset()?;

    let genesis = ts.genesis(&store)?;
    let network = NetworkChain::from_genesis_or_devnet_placeholder(genesis.cid());

    let timestamp = genesis.timestamp;
    let chain_index = ChainIndex::new(Arc::clone(&store));
    let chain_config = ChainConfig::from_chain(&network);
    if chain_config.is_testnet() {
        CurrentNetwork::set_global(Network::Testnet);
    }
    let beacon = Arc::new(chain_config.get_beacon_schedule(timestamp));
    let tipset = chain_index
        .tipset_by_height(epoch, Arc::new(ts), ResolveNullTipset::TakeOlder)
        .with_context(|| format!("couldn't get a tipset at height {epoch}"))?;

    let mut message_calls = vec![];

    let StateOutput { state_root, .. } = apply_block_messages(
        timestamp,
        Arc::new(chain_index),
        Arc::new(chain_config),
        beacon,
        &GLOBAL_MULTI_ENGINE,
        tipset,
        if json {
            Some(|ctx: MessageCallbackCtx<'_>| {
                message_calls.push((
                    ctx.message.clone(),
                    ctx.apply_ret.clone(),
                    ctx.at,
                    ctx.duration,
                ));
                Ok(())
            })
        } else {
            None
        },
        match json {
            true => VMTrace::Traced,
            false => VMTrace::NotTraced,
        }, // enable traces if json flag is used
    )?;

    if json {
        println!("{:#}", structured::json(state_root, message_calls)?);
    } else {
        println!("computed state cid: {state_root}");
    }

    Ok(())
}

mod structured {
    use cid::Cid;
    use serde_json::json;

    use crate::lotus_json::HasLotusJson as _;
    use crate::state_manager::utils::structured;
    use crate::{
        interpreter::CalledAt,
        message::{ChainMessage, Message as _},
        shim::executor::ApplyRet,
    };
    use std::time::Duration;

    pub fn json(
        state_root: Cid,
        contexts: Vec<(ChainMessage, ApplyRet, CalledAt, Duration)>,
    ) -> anyhow::Result<serde_json::Value> {
        Ok(json!({
        "Root": state_root.into_lotus_json(),
        "Trace": contexts
            .into_iter()
            .map(|(message, apply_ret, called_at, duration)| call_json(message, apply_ret, called_at, duration))
            .collect::<Result<Vec<_>, _>>()?
        }))
    }

    fn call_json(
        chain_message: ChainMessage,
        apply_ret: ApplyRet,
        called_at: CalledAt,
        duration: Duration,
    ) -> anyhow::Result<serde_json::Value> {
        let is_explicit = matches!(called_at.apply_kind(), fvm3::executor::ApplyKind::Explicit);

        let chain_message_cid = chain_message.cid();
        let unsigned_message_cid = chain_message.message().cid();

        Ok(json!({
            "MsgCid": chain_message_cid.into_lotus_json(),
            "Msg": chain_message.message().clone().into_lotus_json(),
            "MsgRct": apply_ret.msg_receipt().into_lotus_json(),
            "Error": apply_ret.failure_info().unwrap_or_default(),
            "GasCost": {
                "Message": is_explicit.then_some(unsigned_message_cid.into_lotus_json()),
                "GasUsed": if is_explicit { apply_ret.msg_receipt().gas_used() } else { Default::default() },
                "BaseFeeBurn": apply_ret.base_fee_burn().into_lotus_json(),
                "OverEstimationBurn": apply_ret.over_estimation_burn().into_lotus_json(),
                "MinerPenalty": apply_ret.penalty().into_lotus_json(),
                "MinerTip": apply_ret.miner_tip().into_lotus_json(),
                "Refund": apply_ret.refund().into_lotus_json(),
                "TotalCost": (chain_message.message().required_funds() - &apply_ret.refund()).into_lotus_json(),
            },
            "ExecutionTrace": structured::parse_events(apply_ret.exec_trace())?.into_lotus_json(),
            "Duration": duration.as_nanos().clamp(0, u64::MAX as u128) as u64,
        }))
    }
}
