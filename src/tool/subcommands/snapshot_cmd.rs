// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;
use crate::blocks::Tipset;
use crate::chain::index::{ChainIndex, ResolveNullTipset};
use crate::cli_shared::snapshot;
use crate::daemon::bundle::load_actor_bundles;
use crate::db::car::forest::DEFAULT_FOREST_CAR_FRAME_SIZE;
use crate::db::car::{AnyCar, ManyCar};
use crate::interpreter::{MessageCallbackCtx, VMTrace};
use crate::ipld::{recurse_links_hash, CidHashSet};
use crate::networks::{calibnet, mainnet, ChainConfig, NetworkChain};
use crate::shim::address::CurrentNetwork;
use crate::shim::clock::ChainEpoch;
use crate::shim::machine::MultiEngine;
use crate::state_manager::apply_block_messages;
use crate::utils::db::car_stream::CarStream;
use crate::utils::proofs_api::paramfetch::ensure_params_downloaded;
use anyhow::{bail, Context, Result};
use cid::Cid;
use clap::Subcommand;
use dialoguer::{theme::ColorfulTheme, Confirm};
use futures::TryStreamExt;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared3::address::Network;
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
    pub async fn run(self) -> Result<()> {
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

fn print_computed_state(snapshot: PathBuf, epoch: ChainEpoch, json: bool) -> anyhow::Result<()> {
    // Initialize Blockstore
    let store = Arc::new(AnyCar::try_from(snapshot.as_path())?);

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

    let mut message_calls = vec![];

    let (state_root, _) = apply_block_messages(
        timestamp,
        Arc::new(chain_index),
        Arc::new(chain_config),
        beacon,
        &MultiEngine::default(),
        tipset,
        Some(|ctx: &MessageCallbackCtx| {
            message_calls.push((ctx.message.clone(), ctx.apply_ret.clone(), ctx.at));
            anyhow::Ok(())
        }),
        match json {
            true => VMTrace::Traced,
            false => VMTrace::NotTraced,
        }, // enable traces if json flag is used
    )?;

    if json {
        println!("{:#}", structured::json(state_root, message_calls)?);
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
        message::{ChainMessage, Message as _},
        shim::{
            address::Address,
            error::ExitCode,
            executor::ApplyRet,
            gas::GasCharge,
            kernel::{ErrorNumber, SyscallError},
            trace::{Call, CallReturn, ExecutionEvent},
        },
    };
    use fvm_ipld_encoding::{ipld_block::IpldBlock, RawBytes};
    use itertools::Either;

    pub fn json(
        state_root: Cid,
        contexts: Vec<(ChainMessage, ApplyRet, CalledAt)>,
    ) -> anyhow::Result<serde_json::Value> {
        Ok(json!({
        "Root": LotusJson(state_root),
        "Trace": contexts
            .into_iter()
            .map(|(message, apply_ret, called_at)| call_json(message, apply_ret, called_at))
            .collect::<Result<Vec<_>, _>>()?
        }))
    }

    fn call_json(
        chain_message: ChainMessage,
        apply_ret: ApplyRet,
        called_at: CalledAt,
    ) -> anyhow::Result<serde_json::Value> {
        use crate::lotus_json::Stringify;

        let is_explicit = matches!(called_at.apply_kind(), fvm3::executor::ApplyKind::Explicit);

        let chain_message_cid = chain_message.cid()?;
        let unsiged_message_cid = chain_message.message().cid()?;

        Ok(json!({
            "MsgCid": LotusJson(chain_message_cid),
            "Msg": LotusJson(chain_message.message().clone()),
            "MsgRct": LotusJson(apply_ret.msg_receipt()),
            "Error": apply_ret.failure_info().unwrap_or_default(),
            "GasCost": {
                "Message": is_explicit.then_some(LotusJson(unsiged_message_cid)),
                "GasUsed": is_explicit.then_some(Stringify(apply_ret.msg_receipt().gas_used())).unwrap_or_default(),
                "BaseFeeBurn": LotusJson(apply_ret.base_fee_burn()),
                "OverEstimationBurn": LotusJson(apply_ret.over_estimation_burn()),
                "MinerPenalty": LotusJson(apply_ret.penalty()),
                "MinerTip": LotusJson(apply_ret.miner_tip()),
                "Refund": LotusJson(apply_ret.refund()),
                "TotalCost": LotusJson(chain_message.message().required_funds() - &apply_ret.refund())
            },
            "ExecutionTrace": parse_events(apply_ret.exec_trace())?.map(CallTree::json),
            // Only include timing fields for an easier diff with lotus
            "Duration": null,
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
    fn parse_events(events: Vec<ExecutionEvent>) -> Result<Option<CallTree>, BuildCallTreeError> {
        let mut events = VecDeque::from(events);
        let mut front_load_me = vec![];
        let mut call_trees = vec![];

        // we don't use a `for` loop so we can pass events them to inner parsers
        while let Some(event) = events.pop_front() {
            match event {
                ExecutionEvent::GasCharge(gc) => front_load_me.push(gc),
                ExecutionEvent::Call(call) => call_trees.push(CallTree::parse(call, {
                    // if CallTree::parse took impl Iterator<Item = ExecutionEvent>
                    // the compiler would infinitely recurse trying to resolve
                    // &mut &mut &mut ..: Iterator
                    // so use a VecDeque instead
                    for gc in front_load_me.drain(..).rev() {
                        events.push_front(ExecutionEvent::GasCharge(gc))
                    }
                    &mut events
                })?),
                ExecutionEvent::CallReturn(_)
                | ExecutionEvent::CallAbort(_)
                | ExecutionEvent::CallError(_) => return Err(BuildCallTreeError::UnexpectedReturn),
                ExecutionEvent::Log(_ignored) => {}
                ExecutionEvent::Unknown(u) => {
                    return Err(BuildCallTreeError::UnrecognisedEvent(Box::new(u)))
                }
            }
        }

        if !front_load_me.is_empty() {
            tracing::warn!(
                "vm tracing: ignoring {} trailing gas charges",
                front_load_me.len()
            );
        }

        match call_trees.len() {
            0 => Ok(None),
            1 => Ok(Some(call_trees.remove(0))),
            many => {
                tracing::warn!(
                    "vm tracing: ignoring {} call trees at the root level",
                    many - 1
                );
                Ok(Some(call_trees.remove(0)))
            }
        }
    }

    struct CallTree {
        call: Call,
        gas_charges: Vec<GasCharge>,
        sub_calls: Vec<CallTree>,
        r#return: CallTreeReturn,
    }

    impl CallTree {
        fn json(self) -> serde_json::Value {
            use fvm_shared3::error::ExitCode;

            let Self {
                call:
                    Call {
                        from,
                        to,
                        method_num,
                        params,
                        value,
                        gas_limit: _,
                        read_only: _,
                    },
                gas_charges,
                sub_calls,
                r#return,
            } = self;

            fn params_to_codec_and_data(
                params: Either<RawBytes, Option<IpldBlock>>,
            ) -> (u64, Vec<u8>) {
                params
                    .map_either(
                        // This is more of a guess than anything
                        |raw_bytes| (fvm_ipld_encoding::IPLD_RAW, Vec::from(raw_bytes)),
                        |maybe_ipld| {
                            let IpldBlock { codec, data } = maybe_ipld.unwrap_or_default();
                            (codec, data)
                        },
                    )
                    .into_inner()
            }

            let (codec, data) = params_to_codec_and_data(params);
            let (return_code, return_data, return_codec) = match r#return {
                CallTreeReturn::Return(CallReturn { exit_code, data }) => {
                    let (codec, data) = params_to_codec_and_data(data);
                    (
                        exit_code.map(|it| it.value()).unwrap_or_default(),
                        data,
                        codec,
                    )
                }
                CallTreeReturn::Abort(exit_code) => (exit_code.value(), vec![], 0),
                CallTreeReturn::Error(SyscallError { message: _, number }) => {
                    // Ported from: https://github.com/filecoin-project/filecoin-ffi/blob/v1.23.0/rust/src/fvm/machine.rs#L440
                    let code = match number {
                        ErrorNumber::InsufficientFunds => ExitCode::SYS_INSUFFICIENT_FUNDS.value(),
                        ErrorNumber::NotFound => ExitCode::SYS_INVALID_RECEIVER.value(),
                        _ => ExitCode::SYS_ASSERTION_FAILED.value(),
                    };
                    (code, vec![], 0)
                }
            };

            json!({
                "Msg": {
                    "From": LotusJson(Address::new_id(from)),
                    "To": LotusJson(to),
                    "Value": LotusJson(value),
                    "Method": LotusJson(method_num),
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
            call: Call,
            events: &mut VecDeque<ExecutionEvent>,
        ) -> Result<Self, BuildCallTreeError> {
            let mut gas_charges = vec![];
            let mut sub_calls = vec![];

            // we don't use a for loop over `events` so we can pass them to recursive calls
            while let Some(event) = events.pop_front() {
                let found_return = match event {
                    ExecutionEvent::GasCharge(gc) => {
                        gas_charges.push(gc);
                        None
                    }
                    ExecutionEvent::Call(call) => {
                        sub_calls.push(Self::parse(call, events)?);
                        None
                    }
                    ExecutionEvent::CallReturn(ret) => Some(CallTreeReturn::Return(ret)),
                    ExecutionEvent::CallAbort(ab) => Some(CallTreeReturn::Abort(ab)),
                    ExecutionEvent::CallError(e) => Some(CallTreeReturn::Error(e)),
                    ExecutionEvent::Log(_ignored) => None,
                    // RUST: This should be caught at compile time with #[deny(non_exhaustive_omitted_patterns)]
                    //       So that BuildCallTreeError::UnrecognisedEvent is never constructed
                    //       But that lint is not yet stabilised: https://github.com/rust-lang/rust/issues/89554
                    ExecutionEvent::Unknown(u) => {
                        return Err(BuildCallTreeError::UnrecognisedEvent(Box::new(u)))
                    }
                };

                // commonise the return branch
                if let Some(r#return) = found_return {
                    return Ok(Self {
                        call,
                        gas_charges,
                        sub_calls,
                        r#return,
                    });
                }
            }

            Err(BuildCallTreeError::NoReturn)
        }
    }

    fn gas_charge_json(gc: GasCharge) -> serde_json::Value {
        json!({
            "Name": gc.name(),
            // total gas
            "tg": gc.total().round_up(),
            "cg": gc.compute_gas().round_up(),
            "sg": gc.other_gas().round_up(),
            "tt": null,
        })
    }

    enum CallTreeReturn {
        Return(CallReturn),
        Abort(ExitCode),
        Error(SyscallError),
    }

    #[derive(Debug, thiserror::Error)]
    enum BuildCallTreeError {
        #[error("every ExecutionEvent::Return | ExecutionEvent::CallError should be preceded by an ExecutionEvent::Call, but this one wasn't")]
        UnexpectedReturn,
        #[error("every ExecutionEvent::Call should have a corresponding ExecutionEvent::Return, but this one didn't")]
        NoReturn,
        #[error("unrecognised ExecutionEvent variant: {0:?}")]
        UnrecognisedEvent(Box<dyn std::fmt::Debug + Send + Sync + 'static>),
    }
}
