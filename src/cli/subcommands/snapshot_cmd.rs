// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::blocks::Tipset;
use crate::chain::index::{ChainIndex, ResolveNullTipset};
use crate::cli::subcommands::{cli_error_and_die, handle_rpc_err};
use crate::cli_shared::snapshot::{self, TrustedVendor};
use crate::daemon::bundle::load_actor_bundles;
use crate::db::car::AnyCar;
use crate::db::car::ManyCar;
use crate::interpreter::{CalledAt, VMTrace};
use crate::ipld::{recurse_links_hash, CidHashSet};
use crate::message::ChainMessage;
use crate::networks::{ChainConfig, NetworkChain};
use crate::rpc_api::chain_api::ChainExportParams;
use crate::rpc_client::chain_ops::*;
use crate::shim::address::{CurrentNetwork, Network};
use crate::shim::clock::ChainEpoch;
use crate::shim::executor::ApplyRet;
use crate::shim::machine::MultiEngine;
use crate::state_manager::apply_block_messages;
use crate::utils::db::car_stream::CarStream;
use crate::utils::proofs_api::paramfetch::ensure_params_downloaded;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use clap::Subcommand;
use dialoguer::{theme::ColorfulTheme, Confirm};
use futures::TryStreamExt;
use fvm_ipld_blockstore::Blockstore;
use human_repr::HumanCount;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Subcommand)]
pub enum SnapshotCommands {
    /// Export a snapshot of the chain to `<output_path>`
    Export {
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
        /// How many state-roots to include. Lower limit is 900 for `calibnet` and `mainnet`.
        #[arg(short, long)]
        depth: Option<crate::chain::ChainEpochDelta>,
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
        #[arg(required = true)]
        snapshot_files: Vec<PathBuf>,
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
        output: PathBuf,
        #[arg(long, default_value_t = 3)]
        compression_level: u16,
        /// End zstd frames after they exceed this length
        #[arg(long, default_value_t = 8000usize.next_power_of_two())]
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
    pub async fn run(self, config: Config) -> Result<()> {
        match self {
            Self::Export {
                output_path,
                skip_checksum,
                dry_run,
                tipset,
                depth,
            } => {
                let chain_head = match chain_head(&config.client.rpc_token).await {
                    Ok(LotusJson(head)) => head,
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
                        true,
                    )),
                    false => output_path.clone(),
                };

                let output_dir = output_path.parent().context("invalid output path")?;
                let temp_path = NamedTempFile::new_in(output_dir)?.into_temp_path();

                let params = ChainExportParams {
                    epoch,
                    recent_roots: depth.unwrap_or(config.chain.recent_state_roots),
                    output_path: temp_path.to_path_buf(),
                    tipset_keys: chain_head.key().clone(),
                    skip_checksum,
                    dry_run,
                };

                let finality = config.chain.policy.chain_finality.min(epoch);
                if params.recent_roots < finality {
                    bail!(
                        "For {}, depth has to be at least {finality}.",
                        config.chain.network
                    );
                }

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
            Self::Compress {
                source,
                output,
                compression_level,
                frame_size,
                force,
            } => {
                // If input is 'snapshot.car.zst' and output is '.', set the
                // destination to './snapshot.forest.car.zst'.
                let destination = match output.is_dir() {
                    true => {
                        let mut destination = output;
                        destination.push(source.clone());
                        while let Some(ext) = destination.extension() {
                            if !(ext == "zst" || ext == "car" || ext == "forest") {
                                break;
                            }
                            destination.set_extension("");
                        }
                        destination.with_extension("forest.car.zst")
                    }
                    false => output.clone(),
                };

                if destination.exists() && !force {
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

                println!("Generating ForestCAR.zst file: {:?}", &destination);

                let file = File::open(&source).await?;
                let pb = ProgressBar::new(file.metadata().await?.len()).with_style(
                    ProgressStyle::with_template("{bar} {percent}%, eta: {eta}")
                        .expect("infallible"),
                );
                let file = tokio::io::BufReader::new(pb.wrap_async_read(file));

                let mut block_stream = CarStream::new(file).await?;
                let roots = std::mem::take(&mut block_stream.header.roots);

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
            Self::ComputeState {
                snapshot,
                epoch,
                json,
            } => print_computed_state(snapshot, epoch, json).await,
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
    root: Tipset,
    store: Arc<BlockstoreT>,
    check_links: u32,
    check_network: Option<NetworkChain>,
    check_stateroots: u32,
) -> Result<()>
where
    BlockstoreT: Blockstore + Send + Sync + 'static,
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
fn query_network(ts: &Tipset, db: impl Blockstore) -> Result<NetworkChain> {
    let pb = validation_spinner("Identifying genesis block:");

    match ts.genesis(db).and_then(|genesis| {
        NetworkChain::from_genesis(genesis.cid())
            .context("genesis block does not match known calibnet or mainnet genesis blocks")
    }) {
        Ok(devnet_or_mainnet) => {
            pb.abandon_with_message("✅ found!");
            Ok(devnet_or_mainnet)
        }
        Err(e) => {
            pb.finish_with_message("❌ failed!");
            Err(e)
        }
    }
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
) -> Result<()>
where
    DB: Blockstore + Send + Sync + 'static,
{
    let chain_config = Arc::new(ChainConfig::from_chain(&network));
    let genesis = ts.genesis(db)?;

    let pb = validation_spinner("Running tipset transactions:").with_finish(
        indicatif::ProgressFinish::AbandonWithMessage(
            "❌ Transaction result differs from Lotus!".into(),
        ),
    );

    let last_epoch = ts.epoch() - epochs as i64;

    // Bundles are required when doing state migrations.
    load_actor_bundles(&db).await?;

    // Set proof parameter data dir and make sure the proofs are available
    crate::utils::proofs_api::paramfetch::set_proofs_parameter_cache_dir_env(
        &Config::default().client.data_dir,
    );

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

async fn print_computed_state(
    snapshot: PathBuf,
    epoch: ChainEpoch,
    json: bool,
) -> anyhow::Result<()> {
    // Initialize Blockstore
    let store = Arc::new(AnyCar::try_from(snapshot)?);

    // Prepare call to apply_block_messages
    let ts = store.heaviest_tipset()?;

    let genesis = ts.genesis(&store)?;
    let network = NetworkChain::from_genesis_or_devnet_placeholder(genesis.cid());

    let timestamp = genesis.timestamp();
    let chain_index = ChainIndex::new(Arc::clone(&store));
    let chain_config = ChainConfig::from_chain(&network);
    if chain_config.is_testnet() {
        CurrentNetwork::set_global(Network::Testnet);
    }
    let beacon = Arc::new(chain_config.get_beacon_schedule(timestamp));
    let tipset = chain_index
        .tipset_by_height(epoch, Arc::new(ts), ResolveNullTipset::TakeOlder)
        .context(format!("couldn't get a tipset at height {}", epoch))?;

    let mut contexts = vec![];

    let (state_root, _) = apply_block_messages(
        timestamp,
        Arc::new(chain_index),
        Arc::new(chain_config),
        beacon,
        &MultiEngine::default(),
        tipset,
        Some(
            |_: &Cid, message: &ChainMessage, ret: &ApplyRet, at: CalledAt| {
                contexts.push((message.clone(), ret.clone(), at));
                anyhow::Ok(())
            },
        ),
        match json {
            true => VMTrace::Traced,
            false => VMTrace::NotTraced,
        }, // enable traces if json flag is used
    )?;

    if json {
        println!("{:#}", structured::json(state_root, contexts)?);
    } else {
        println!("computed state cid: {}", state_root);
    }

    Ok(())
}

/// Parsed tree of [`fvm3::trace::ExecutionEvent`]s
mod structured {
    use std::collections::VecDeque;

    use cid::Cid;
    use serde_json::json;

    use crate::{
        interpreter::CalledAt,
        lotus_json::LotusJson,
        message::{ChainMessage, Message as _}, // JANK(aatifsyed): Message is overloaded
        shim::{address::Address, econ::TokenAmount, executor::ApplyRet},
    };
    use fvm_ipld_encoding::ipld_block::IpldBlock;

    pub fn json(
        state_root: Cid,
        contexts: Vec<(ChainMessage, ApplyRet, CalledAt)>,
    ) -> anyhow::Result<serde_json::Value> {
        Ok(json!({
        "Root": LotusJson(state_root),
        "Trace": contexts
            .into_iter()
            .map(|(message, apply_ret, called_at)| context_json(message, apply_ret, called_at))
            .collect::<Result<Vec<_>, _>>()?
        }))
    }

    fn context_json(
        chain_message: ChainMessage,
        apply_ret: ApplyRet,
        called_at: CalledAt,
    ) -> anyhow::Result<serde_json::Value> {
        use crate::lotus_json::Stringify;

        // TODO(aatifsyed): what does this even mean
        let is_explicit = match called_at {
            CalledAt::Applied => true,
            CalledAt::Reward | CalledAt::Cron => false,
        };

        let chain_message_cid = chain_message.cid()?;
        let unsiged_message_cid = chain_message.message().cid()?;

        Ok(json!({
            "MsgCid": LotusJson(chain_message_cid),
            "Msg": LotusJson(chain_message.message().clone()),
            "MsgRct": LotusJson(apply_ret.msg_receipt()),
            // TODO(aatifsyed): ^ this should include the "EventsRoot": null
            //                  but LotusJson<Receipt> currently ignores that field
            "Error": apply_ret.failure_info().unwrap_or_default(),
            "GasCost": {
                "Message": is_explicit.then_some(LotusJson(unsiged_message_cid)),
                "GasUsed": is_explicit.then_some(Stringify(apply_ret.msg_receipt().gas_used())),
                "BaseFeeBurn": LotusJson(apply_ret.base_fee_burn()),
                "OverEstimationBurn": LotusJson(apply_ret.over_estimation_burn()),
                "MinerPenalty": LotusJson(apply_ret.penalty()),
                "MinerTip": LotusJson(apply_ret.miner_tip()),
                "Refund": LotusJson(apply_ret.refund()),
                "TotalCost": LotusJson(chain_message.message().required_funds() - &apply_ret.refund()) // JANK(aatifsyed): shouldn't need to borrow &TokenAmount for Sub
            },
            "ExecutionTrace": parse_events(apply_ret.exec_trace())?.map(CallTree::json)
            // "Duration": unimplemented!(),
        }))
    }

    /// Construct a single [`CallTree`]s from a linear array of [`ExecutionEvent`](fvm3::trace::ExecutionEvent)s.
    ///
    /// This function is so-called because it similar to the parse step in a traditional compiler:
    /// ```text
    /// text --lex-->     tokens     --parse-->   AST
    ///               ExecutionEvent --parse--> CallTree
    /// ```
    ///
    /// This function is notable in that [`GasCharge`](fvm3::gas::GasCharge)s which precede a [`CallTree`] at the root level
    /// are attributed to that node.
    ///
    /// We call this "front loading", and is copied from [this (rather obscure) code in `filecoin-ffi`](https://github.com/filecoin-project/filecoin-ffi/blob/v1.23.0/rust/src/fvm/machine.rs#L209)
    ///
    /// ```text
    /// GasCharge GasCharge Call GasCharge Call CallError CallReturn
    /// ────┬──── ────┬──── ─┬── ────┬──── ─┬── ───┬───── ────┬─────
    ///     │         │      │       │      │      │          │
    ///     │         │      │       │      └─(T)──┘          │
    ///     │         │      └───────┴───(T)───┴──────────────┘
    ///     └─────────┴──────────────────►│
    ///     ("front loaded" GasCharges)   │
    ///                                  (T)
    ///
    /// (T): a CallTree node
    /// ```
    ///
    /// Multiple call trees and trailing gas will be warned and ignored.
    /// If no call tree is found, returns [`Ok(None)`]
    fn parse_events(
        events: Vec<fvm3::trace::ExecutionEvent>,
    ) -> Result<Option<CallTree>, BuildCallTreeError> {
        let mut events = VecDeque::from(events);
        let mut root_gas_charges = vec![];
        let mut call_trees = vec![];

        // we don't use a `for` loop so we can pass events them to inner parsers
        while let Some(event) = events.pop_front() {
            match event {
                fvm3::trace::ExecutionEvent::GasCharge(gas_charge) => {
                    root_gas_charges.push(gas_charge)
                }
                fvm3::trace::ExecutionEvent::Call {
                    from,
                    to,
                    method,
                    params,
                    value,
                } => call_trees.push(CallTree::parse(
                    ExecutionEventCall {
                        from,
                        to,
                        method,
                        params,
                        value,
                    },
                    {
                        // if CallTree::parse took impl Iterator<Item = ExecutionEvent>
                        // the compiler would infinitely recurse trying to resolve
                        // &mut &mut &mut ..: Iterator
                        // so use a VecDeque instead
                        for gc in root_gas_charges.drain(..).rev() {
                            events.push_front(fvm3::trace::ExecutionEvent::GasCharge(gc))
                        }
                        &mut events
                    },
                )?),
                fvm3::trace::ExecutionEvent::CallReturn(_, _)
                | fvm3::trace::ExecutionEvent::CallError(_) => {
                    return Err(BuildCallTreeError::UnexpectedReturn)
                }
                unrecognised => return Err(BuildCallTreeError::UnrecognisedEvent(unrecognised)),
            }
        }

        if !root_gas_charges.is_empty() {
            // FIXME(aatifsyed): tracing should go to stderr, but it doesn't.
            //                   this screws up `make_output.bash`, so comment out
            //                   for now.
            eprintln!(
                "vm tracing: ignoring {} trailing gas charges",
                root_gas_charges.len()
            );
        }

        match call_trees.len() {
            0 => Ok(None),
            1 => {
                let mut call_tree = call_trees.remove(0);
                call_tree.gas_charges.extend(root_gas_charges);
                Ok(Some(call_tree))
            }
            many => {
                // FIXME(aatifsyed): as above
                eprintln!(
                    "vm tracing: ignoring {} call trees at the root level",
                    many - 1
                );
                Ok(Some(call_trees.remove(0)))
            }
        }
    }

    struct CallTree {
        call: ExecutionEventCall,
        gas_charges: Vec<fvm3::gas::GasCharge>,
        sub_calls: Vec<CallTree>,
        r#return: CallTreeReturn,
    }

    impl CallTree {
        fn json(self) -> serde_json::Value {
            use fvm_shared3::error::ExitCode;

            let Self {
                call:
                    ExecutionEventCall {
                        from,
                        to,
                        method,
                        params,
                        value,
                    },
                gas_charges,
                sub_calls,
                r#return,
            } = self;

            let IpldBlock { codec, data } = params.unwrap_or_default();
            let (return_code, return_data, return_codec) = match r#return {
                // Ported from: https://github.com/filecoin-project/filecoin-ffi/blob/v1.23.0/rust/src/fvm/machine.rs#L440
                CallTreeReturn::Return(code, data) => {
                    let IpldBlock { codec, data } = data.unwrap_or_default();
                    (code, data, codec)
                }
                CallTreeReturn::SyscallError(fvm3::kernel::SyscallError(_, n)) => {
                    let code = match n {
                        fvm_shared3::error::ErrorNumber::InsufficientFunds => {
                            ExitCode::SYS_INSUFFICIENT_FUNDS
                        }
                        fvm_shared3::error::ErrorNumber::NotFound => ExitCode::SYS_INVALID_RECEIVER,
                        _ => ExitCode::SYS_ASSERTION_FAILED,
                    };
                    (code, vec![], 0)
                }
            };
            json!({
                "Msg": {
                    "From": LotusJson(Address::new_id(from)),
                    "To": LotusJson(Address::from(to)),
                    "Value": LotusJson(TokenAmount::from(value)),
                    "Method": LotusJson(method),
                    "Params": LotusJson(data),
                    "ParamsCodec": LotusJson(codec)
                },
                // "MsgRct" might suggest that this is the right place to use LotusJson<crate::shim::executor::Receipt>
                // But this is actually different information - e.g "GasUsed" isn't shown by Lotus
                // And contructing a Receipt requires RawBytes, which is _not_ the same as the IpldBlock in CallTreeReturn::Return
                "MsgRct": {
                    "ExitCode": LotusJson(return_code),
                    "Return": LotusJson(return_data),
                    "ReturnCodec": LotusJson(return_codec),
                },
                "GasCharges": LotusJson(gas_charges.into_iter().map(gas_charge_json).collect::<Vec<_>>()),
                "Subcalls": LotusJson(sub_calls.into_iter().map(Self::json).collect::<Vec<_>>())
            })
        }
    }

    fn gas_charge_json(gas_charge: fvm3::gas::GasCharge) -> serde_json::Value {
        let fvm3::gas::GasCharge {
            name,
            compute_gas,
            other_gas,
            elapsed,
        } = gas_charge;
        json!({
            "Name": name,
            // total gas
            "tg": (compute_gas + other_gas).round_up(),
            "cg": compute_gas.round_up(),
            "sg": other_gas.round_up(),
            "tt": elapsed.get()
                    .map(std::time::Duration::as_nanos)
                    .unwrap_or(u64::MAX as u128)
        })
    }

    enum CallTreeReturn {
        Return(fvm_shared3::error::ExitCode, Option<IpldBlock>),
        SyscallError(fvm3::kernel::SyscallError),
    }

    /// Fields on [`fvm3::trace::ExecutionEvent::Call`]
    struct ExecutionEventCall {
        from: fvm_shared3::ActorID,
        to: fvm_shared3::address::Address,
        method: fvm_shared3::MethodNum,
        params: Option<fvm_ipld_encoding::ipld_block::IpldBlock>,
        value: fvm_shared3::econ::TokenAmount,
    }

    #[derive(Debug, thiserror::Error)]
    enum BuildCallTreeError {
        #[error("every ExecutionEvent::Return | ExecutionEvent::CallError should be preceded by an ExecutionEvent::Call, but this one wasn't")]
        UnexpectedReturn,
        #[error("every ExecutionEvent::Call should have a corresponding ExecutionEvent::Return, but this one didn't")]
        NoReturn,
        #[error("unrecognised ExecutionEvent variant: {0:?}")]
        UnrecognisedEvent(fvm3::trace::ExecutionEvent),
    }

    impl CallTree {
        /// ```text
        ///    events: GasCharge Call CallError CallReturn ...
        ///            ────┬──── ─┬── ───┬───── ────┬─────
        ///                │      │      │          │
        /// ┌──────┐       │      └─(T)──┘          │
        /// │ Call ├───────┴───(T)───┴──────────────┘
        /// └──────┘            |                   ▲
        ///                     ▼                   │
        ///              Returned CallTree          │
        ///                                     parsing end
        /// ```
        fn parse(
            call: ExecutionEventCall,
            events: &mut VecDeque<fvm3::trace::ExecutionEvent>,
        ) -> Result<Self, BuildCallTreeError> {
            let mut gas_charges = vec![];
            let mut sub_calls = vec![];

            // we don't use a for loop over `events` so we can pass them to recursive calls
            while let Some(event) = events.pop_front() {
                match event {
                    fvm3::trace::ExecutionEvent::GasCharge(gas_charge) => {
                        gas_charges.push(gas_charge)
                    }
                    fvm3::trace::ExecutionEvent::Call {
                        from,
                        to,
                        method,
                        params,
                        value,
                    } => sub_calls.push(Self::parse(
                        ExecutionEventCall {
                            from,
                            to,
                            method,
                            params,
                            value,
                        },
                        events,
                    )?),
                    fvm3::trace::ExecutionEvent::CallReturn(exit_code, data) => {
                        return Ok(Self {
                            call,
                            gas_charges,
                            sub_calls,
                            r#return: CallTreeReturn::Return(exit_code, data),
                        })
                    }
                    fvm3::trace::ExecutionEvent::CallError(syscall_error) => {
                        return Ok(Self {
                            call,
                            gas_charges,
                            sub_calls,
                            r#return: CallTreeReturn::SyscallError(syscall_error),
                        })
                    }
                    // RUST: This should be caught at compile time with #[deny(non_exhaustive_omitted_patterns)]
                    //       So that BuildCallTreeError::UnrecognisedEvent is never constructed
                    //       But that lint is not yet stabilised: https://github.com/rust-lang/rust/issues/89554
                    unrecognised => {
                        return Err(BuildCallTreeError::UnrecognisedEvent(unrecognised))
                    }
                }
            }

            Err(BuildCallTreeError::NoReturn)
        }
    }
}
