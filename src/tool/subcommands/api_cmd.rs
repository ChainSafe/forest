// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::{ElectionProof, Ticket, Tipset};
use crate::chain::ChainStore;
use crate::chain_sync::{SyncConfig, SyncStage};
use crate::cli_shared::snapshot::TrustedVendor;
use crate::daemon::db_util::{download_to, populate_eth_mappings};
use crate::db::{car::ManyCar, MemoryDB};
use crate::eth::EthChainId as EthChainIdType;
use crate::genesis::{get_network_name_from_genesis, read_genesis_header};
use crate::key_management::{KeyStore, KeyStoreConfig};
use crate::lotus_json::HasLotusJson;
use crate::message::{Message as _, SignedMessage};
use crate::message_pool::{MessagePool, MpoolRpcProvider};
use crate::networks::{ChainConfig, NetworkChain};
use crate::rpc::beacon::BeaconGetEntry;
use crate::rpc::eth::types::{EthAddress, EthBytes};
use crate::rpc::gas::GasEstimateGasLimit;
use crate::rpc::miner::BlockTemplate;
use crate::rpc::state::StateGetAllClaims;
use crate::rpc::types::{ApiTipsetKey, MessageFilter, MessageLookup};
use crate::rpc::{
    self,
    eth::{types::*, *},
};
use crate::rpc::{prelude::*, start_rpc, RPCState};
use crate::shim::actors::MarketActorStateLoad as _;
use crate::shim::address::{CurrentNetwork, Network};
use crate::shim::{
    address::{Address, Protocol},
    crypto::Signature,
    econ::TokenAmount,
    message::{Message, METHOD_SEND},
    state_tree::StateTree,
};
use crate::state_manager::StateManager;
use crate::utils::UrlFromMultiAddr;
use ahash::HashMap;
use anyhow::{bail, Context as _};
use bls_signatures::Serialize as _;
use cid::Cid;
use clap::{Subcommand, ValueEnum};
use fil_actor_interface::market;
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use fil_actors_shared::v10::runtime::DomainSeparationTag;
use futures::{stream::FuturesUnordered, StreamExt};
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools as _;
use jsonrpsee::types::ErrorCode;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use similar::{ChangeTag, TextDiff};
use std::io;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    time::Duration,
};
use tabled::{builder::Builder, settings::Style};
use tokio::{
    signal::{
        ctrl_c,
        unix::{signal, SignalKind},
    },
    sync::{mpsc, RwLock, Semaphore},
    task::JoinSet,
};
use tracing::{debug, info, warn};
use types::BlockNumberOrPredefined;

const COLLECTION_SAMPLE_SIZE: usize = 5;

const CALIBNET_CHAIN_ID: EthChainIdType = crate::networks::calibnet::ETH_CHAIN_ID;

#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum ApiCommands {
    // Serve
    Serve {
        /// Snapshot input paths. Supports `.car`, `.car.zst`, and `.forest.car.zst`.
        snapshot_files: Vec<PathBuf>,
        /// Filecoin network chain
        #[arg(long, default_value = "mainnet")]
        chain: NetworkChain,
        // RPC port
        #[arg(long, default_value_t = crate::rpc::DEFAULT_PORT)]
        port: u16,
        // Allow downloading snapshot automatically
        #[arg(long)]
        auto_download_snapshot: bool,
        /// Validate snapshot at given EPOCH, use a negative value -N to validate
        /// the last N EPOCH(s) starting at HEAD.
        #[arg(long, default_value_t = -50)]
        height: i64,
    },
    /// Compare two RPC providers.
    ///
    /// The providers are labeled `forest` and `lotus`,
    /// but other nodes may be used (such as `venus`).
    ///
    /// The `lotus` node is assumed to be correct and the `forest` node will be
    /// marked as incorrect if it deviates.
    ///
    /// If snapshot files are provided,
    /// these files will be used to generate additional tests.
    ///
    /// Example output:
    /// ```markdown
    /// | RPC Method                        | Forest              | Lotus         |
    /// |-----------------------------------|---------------------|---------------|
    /// | Filecoin.ChainGetBlock            | Valid               | Valid         |
    /// | Filecoin.ChainGetGenesis          | Valid               | Valid         |
    /// | Filecoin.ChainGetMessage (67)     | InternalServerError | Valid         |
    /// ```
    /// The number after a method name indicates how many times an RPC call was tested.
    Compare {
        /// Forest address
        #[clap(long, default_value = "/ip4/127.0.0.1/tcp/2345/http")]
        forest: UrlFromMultiAddr,
        /// Lotus address
        #[clap(long, default_value = "/ip4/127.0.0.1/tcp/1234/http")]
        lotus: UrlFromMultiAddr,
        /// Filter which tests to run according to method name. Case sensitive.
        #[arg(long, default_value = "")]
        filter: String,
        /// Filter file which tests to run according to method name. Case sensitive.
        /// The file should contain one entry per line. Lines starting with `!`
        /// are considered as rejected methods, while the others are allowed.
        /// Empty lines and lines starting with `#` are ignored.
        #[arg(long)]
        filter_file: Option<PathBuf>,
        /// Cancel test run on the first failure
        #[arg(long)]
        fail_fast: bool,

        #[arg(long, value_enum, default_value_t = RunIgnored::Default)]
        /// Behavior for tests marked as `ignored`.
        run_ignored: RunIgnored,
        /// Maximum number of concurrent requests
        #[arg(long, default_value = "8")]
        max_concurrent_requests: usize,

        #[command(flatten)]
        create_tests_args: CreateTestsArgs,
    },
    DumpTests {
        #[command(flatten)]
        create_tests_args: CreateTestsArgs,
        /// Which API path to dump.
        #[arg(long)]
        path: rpc::ApiPath,
        #[arg(long)]
        include_ignored: bool,
    },
}

impl ApiCommands {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Serve {
                snapshot_files,
                chain,
                port,
                auto_download_snapshot,
                height,
            } => {
                start_offline_server(snapshot_files, chain, port, auto_download_snapshot, height)
                    .await?;
            }
            Self::Compare {
                forest: UrlFromMultiAddr(forest),
                lotus: UrlFromMultiAddr(lotus),
                filter,
                filter_file,
                fail_fast,
                run_ignored,
                max_concurrent_requests,
                create_tests_args,
            } => {
                let forest = rpc::Client::from_url(forest);
                let lotus = rpc::Client::from_url(lotus);

                let tests = create_tests(create_tests_args)?;
                run_tests(
                    tests,
                    forest,
                    lotus,
                    max_concurrent_requests,
                    filter_file,
                    filter,
                    run_ignored,
                    fail_fast,
                )
                .await?
            }
            Self::DumpTests {
                create_tests_args,
                path,
                include_ignored,
            } => {
                for RpcTest {
                    request:
                        rpc::Request {
                            method_name,
                            params,
                            api_paths,
                            ..
                        },
                    ignore,
                    ..
                } in create_tests(create_tests_args)?
                {
                    if !api_paths.contains(path) {
                        continue;
                    }
                    if ignore.is_some() && !include_ignored {
                        continue;
                    }

                    let dialogue = Dialogue {
                        method: method_name.into(),
                        params: match params {
                            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
                                bail!("params may not be a primitive")
                            }
                            Value::Array(v) => {
                                Some(ez_jsonrpc_types::RequestParameters::ByPosition(v))
                            }
                            Value::Object(it) => Some(ez_jsonrpc_types::RequestParameters::ByName(
                                it.into_iter().collect(),
                            )),
                        },
                        response: None,
                    };
                    serde_json::to_writer(io::stdout(), &dialogue)?;
                    println!();
                }
            }
        }
        Ok(())
    }
}

#[derive(clap::Args, Debug)]
pub struct CreateTestsArgs {
    /// The number of tipsets to use to generate test cases.
    #[arg(short, long, default_value = "10")]
    n_tipsets: usize,
    /// Miner address to use for miner tests. Miner worker key must be in the key-store.
    #[arg(long)]
    miner_address: Option<Address>,
    /// Worker address to use where key is applicable. Worker key must be in the key-store.
    #[arg(long)]
    worker_address: Option<Address>,
    /// Ethereum chain ID. Default to the calibnet chain ID.
    #[arg(long, default_value_t = CALIBNET_CHAIN_ID)]
    eth_chain_id: EthChainIdType,
    /// Snapshot input paths. Supports `.car`, `.car.zst`, and `.forest.car.zst`.
    snapshot_files: Vec<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Dialogue {
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<ez_jsonrpc_types::RequestParameters>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response: Option<DialogueResponse>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum DialogueResponse {
    Result(Value),
    Error(ez_jsonrpc_types::Error),
}

#[derive(ValueEnum, Debug, Clone)]
#[clap(rename_all = "kebab_case")]
pub enum RunIgnored {
    Default,
    IgnoredOnly,
    All,
}

/// Brief description of a single method call against a single host
#[derive(Debug, Clone, PartialOrd, Ord, PartialEq, Eq, Hash)]
enum TestSummary {
    /// Server spoke JSON-RPC: no such method
    MissingMethod,
    /// Server spoke JSON-RPC: bad request (or other error)
    Rejected(String),
    /// Server doesn't seem to be speaking JSON-RPC
    NotJsonRPC,
    /// Transport or ask task management errors
    InfraError,
    /// Server returned JSON-RPC and it didn't match our schema
    BadJson,
    /// Server returned JSON-RPC and it matched our schema, but failed validation
    CustomCheckFailed,
    Timeout,
    Valid,
}

impl TestSummary {
    fn from_err(err: &rpc::ClientError) -> Self {
        match err {
            rpc::ClientError::Call(it) => match it.code().into() {
                ErrorCode::MethodNotFound => Self::MissingMethod,
                _ => Self::Rejected(it.message().to_string()),
            },
            rpc::ClientError::ParseError(_) => Self::NotJsonRPC,
            rpc::ClientError::RequestTimeout => Self::Timeout,

            rpc::ClientError::Transport(_)
            | rpc::ClientError::RestartNeeded(_)
            | rpc::ClientError::InvalidSubscriptionId
            | rpc::ClientError::InvalidRequestId(_)
            | rpc::ClientError::Custom(_)
            | rpc::ClientError::HttpNotImplemented
            | rpc::ClientError::EmptyBatchRequest(_)
            | rpc::ClientError::RegisterMethod(_) => Self::InfraError,
        }
    }
}

/// Data about a failed test. Used for debugging.
struct TestDump {
    request: rpc::Request,
    forest_response: Option<String>,
    lotus_response: Option<String>,
}

impl std::fmt::Display for TestDump {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Request dump: {:?}", self.request)?;
        writeln!(f, "Request params JSON: {}", self.request.params)?;
        if let (Some(forest_response), Some(lotus_response)) =
            (&self.forest_response, &self.lotus_response)
        {
            let diff = TextDiff::from_lines(forest_response, lotus_response);
            let mut print_diff = Vec::new();
            for change in diff.iter_all_changes() {
                let sign = match change.tag() {
                    ChangeTag::Delete => "-",
                    ChangeTag::Insert => "+",
                    ChangeTag::Equal => " ",
                };
                print_diff.push(format!("{sign}{change}"));
            }
            writeln!(f, "Forest response: {}", forest_response)?;
            writeln!(f, "Lotus response: {}", lotus_response)?;
            writeln!(f, "Diff: {}", print_diff.join("\n"))?;
        } else {
            if let Some(forest_response) = &self.forest_response {
                writeln!(f, "Forest response: {}", forest_response)?;
            }
            if let Some(lotus_response) = &self.lotus_response {
                writeln!(f, "Lotus response: {}", lotus_response)?;
            }
        };
        Ok(())
    }
}

struct TestResult {
    /// Forest result after calling the RPC method.
    forest_status: TestSummary,
    /// Lotus result after calling the RPC method.
    lotus_status: TestSummary,
    /// Optional data dump if either status was invalid.
    test_dump: Option<TestDump>,
}

enum PolicyOnRejected {
    Fail,
    Pass,
    PassWithIdenticalError,
}

struct RpcTest {
    request: rpc::Request,
    check_syntax: Arc<dyn Fn(serde_json::Value) -> bool + Send + Sync>,
    check_semantics: Arc<dyn Fn(serde_json::Value, serde_json::Value) -> bool + Send + Sync>,
    ignore: Option<&'static str>,
    policy_on_rejected: PolicyOnRejected,
}

/// Duplication between `<method>` and `<method>_raw` is a temporary measure, and
/// should be removed when <https://github.com/ChainSafe/forest/issues/4032> is
/// completed.
impl RpcTest {
    /// Check that an endpoint exists and that both the Lotus and Forest JSON
    /// response follows the same schema.
    fn basic<T>(request: rpc::Request<T>) -> Self
    where
        T: HasLotusJson,
    {
        Self::basic_raw(request.map_ty::<T::LotusJson>())
    }
    /// See [Self::basic], and note on this `impl` block.
    fn basic_raw<T: DeserializeOwned>(request: rpc::Request<T>) -> Self {
        Self {
            request: request.map_ty(),
            check_syntax: Arc::new(|it| match serde_json::from_value::<T>(it) {
                Ok(_) => true,
                Err(e) => {
                    debug!(?e);
                    false
                }
            }),
            check_semantics: Arc::new(|_, _| true),
            ignore: None,
            policy_on_rejected: PolicyOnRejected::Fail,
        }
    }
    /// Check that an endpoint exists, has the same JSON schema, and do custom
    /// validation over both responses.
    fn validate<T: HasLotusJson>(
        request: rpc::Request<T>,
        validate: impl Fn(T, T) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self::validate_raw(request.map_ty::<T::LotusJson>(), move |l, r| {
            validate(T::from_lotus_json(l), T::from_lotus_json(r))
        })
    }
    /// See [Self::validate], and note on this `impl` block.
    fn validate_raw<T: DeserializeOwned>(
        request: rpc::Request<T>,
        validate: impl Fn(T, T) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            request: request.map_ty(),
            check_syntax: Arc::new(|value| match serde_json::from_value::<T>(value) {
                Ok(_) => true,
                Err(e) => {
                    debug!("{e}");
                    false
                }
            }),
            check_semantics: Arc::new(move |forest_json, lotus_json| {
                match (
                    serde_json::from_value::<T>(forest_json),
                    serde_json::from_value::<T>(lotus_json),
                ) {
                    (Ok(forest), Ok(lotus)) => validate(forest, lotus),
                    (forest, lotus) => {
                        if let Err(e) = forest {
                            debug!("[forest] invalid json: {e}");
                        }
                        if let Err(e) = lotus {
                            debug!("[lotus] invalid json: {e}");
                        }
                        false
                    }
                }
            }),
            ignore: None,
            policy_on_rejected: PolicyOnRejected::Fail,
        }
    }
    /// Check that an endpoint exists and that Forest returns exactly the same
    /// JSON as Lotus.
    fn identity<T: PartialEq + HasLotusJson>(request: rpc::Request<T>) -> RpcTest {
        Self::validate(request, |forest, lotus| forest == lotus)
    }

    fn ignore(mut self, msg: &'static str) -> Self {
        self.ignore = Some(msg);
        self
    }

    fn policy_on_rejected(mut self, policy: PolicyOnRejected) -> Self {
        self.policy_on_rejected = policy;
        self
    }

    async fn run(&self, forest: &rpc::Client, lotus: &rpc::Client) -> TestResult {
        let forest_resp = forest.call(self.request.clone()).await;
        let lotus_resp = lotus.call(self.request.clone()).await;

        let forest_json_str = if let Ok(forest_resp) = forest_resp.as_ref() {
            serde_json::to_string_pretty(forest_resp).ok()
        } else {
            None
        };

        let lotus_json_str = if let Ok(lotus_resp) = lotus_resp.as_ref() {
            serde_json::to_string_pretty(lotus_resp).ok()
        } else {
            None
        };

        let (forest_status, lotus_status) = match (forest_resp, lotus_resp) {
            (Ok(forest), Ok(lotus))
                if (self.check_syntax)(forest.clone()) && (self.check_syntax)(lotus.clone()) =>
            {
                let forest_status = if (self.check_semantics)(forest, lotus) {
                    TestSummary::Valid
                } else {
                    TestSummary::CustomCheckFailed
                };
                (forest_status, TestSummary::Valid)
            }
            (forest_resp, lotus_resp) => {
                let forest_status = forest_resp.map_or_else(
                    |e| TestSummary::from_err(&e),
                    |value| {
                        if (self.check_syntax)(value) {
                            TestSummary::Valid
                        } else {
                            TestSummary::BadJson
                        }
                    },
                );
                let lotus_status = lotus_resp.map_or_else(
                    |e| TestSummary::from_err(&e),
                    |value| {
                        if (self.check_syntax)(value) {
                            TestSummary::Valid
                        } else {
                            TestSummary::BadJson
                        }
                    },
                );

                (forest_status, lotus_status)
            }
        };

        if forest_status == TestSummary::Valid && lotus_status == TestSummary::Valid {
            TestResult {
                forest_status,
                lotus_status,
                test_dump: None,
            }
        } else {
            TestResult {
                forest_status,
                lotus_status,
                test_dump: Some(TestDump {
                    request: self.request.clone(),
                    forest_response: forest_json_str,
                    lotus_response: lotus_json_str,
                }),
            }
        }
    }
}

fn common_tests() -> Vec<RpcTest> {
    vec![
        RpcTest::basic(Version::request(()).unwrap()),
        RpcTest::basic(StartTime::request(()).unwrap()),
        RpcTest::basic(Session::request(()).unwrap()),
    ]
}

fn beacon_tests() -> Vec<RpcTest> {
    vec![RpcTest::identity(
        BeaconGetEntry::request((10101,)).unwrap(),
    )]
}

fn chain_tests() -> Vec<RpcTest> {
    vec![
        RpcTest::basic(ChainHead::request(()).unwrap()),
        RpcTest::identity(ChainGetGenesis::request(()).unwrap()),
    ]
}

fn chain_tests_with_tipset<DB: Blockstore>(
    store: &Arc<DB>,
    tipset: &Tipset,
) -> anyhow::Result<Vec<RpcTest>> {
    let mut tests = vec![
        RpcTest::identity(ChainGetTipSetAfterHeight::request((
            tipset.epoch(),
            Default::default(),
        ))?),
        RpcTest::identity(ChainGetTipSetAfterHeight::request((
            tipset.epoch(),
            Default::default(),
        ))?),
        RpcTest::identity(ChainGetTipSet::request((tipset.key().clone().into(),))?),
        RpcTest::identity(ChainGetPath::request((
            tipset.key().clone(),
            tipset.parents().clone(),
        ))?),
        RpcTest::identity(ChainGetMessagesInTipset::request((tipset
            .key()
            .clone()
            .into(),))?),
        RpcTest::identity(ChainTipSetWeight::request((tipset.key().into(),))?),
    ];

    for block in tipset.block_headers() {
        let block_cid = *block.cid();
        tests.extend([
            RpcTest::identity(ChainReadObj::request((block_cid,))?),
            RpcTest::identity(ChainHasObj::request((block_cid,))?),
            RpcTest::identity(ChainGetBlock::request((block_cid,))?),
            RpcTest::identity(ChainGetBlockMessages::request((block_cid,))?),
            RpcTest::identity(ChainGetParentMessages::request((block_cid,))?),
            RpcTest::identity(ChainGetParentReceipts::request((block_cid,))?),
            RpcTest::identity(ChainStatObj::request((block.messages, None))?),
            RpcTest::identity(ChainStatObj::request((
                block.messages,
                Some(block.messages),
            ))?),
        ]);

        let (bls_messages, secp_messages) = crate::chain::store::block_messages(&store, block)?;
        for msg_cid in sample_message_cids(bls_messages.iter(), secp_messages.iter()) {
            tests.extend([RpcTest::identity(ChainGetMessage::request((msg_cid,))?)]);
        }
    }

    Ok(tests)
}

const TICKET_QUALITY_GREEDY: f64 = 0.9;
const TICKET_QUALITY_OPTIMAL: f64 = 0.8;

fn mpool_tests() -> Vec<RpcTest> {
    vec![
        RpcTest::basic(MpoolPending::request((ApiTipsetKey(None),)).unwrap()),
        RpcTest::basic(MpoolSelect::request((ApiTipsetKey(None), TICKET_QUALITY_GREEDY)).unwrap()),
        RpcTest::basic(MpoolSelect::request((ApiTipsetKey(None), TICKET_QUALITY_OPTIMAL)).unwrap())
            .ignore("https://github.com/ChainSafe/forest/issues/4490"),
    ]
}

fn mpool_tests_with_tipset(tipset: &Tipset) -> Vec<RpcTest> {
    vec![
        RpcTest::basic(MpoolPending::request((tipset.key().into(),)).unwrap()),
        RpcTest::basic(MpoolSelect::request((tipset.key().into(), TICKET_QUALITY_GREEDY)).unwrap()),
        RpcTest::basic(
            MpoolSelect::request((tipset.key().into(), TICKET_QUALITY_OPTIMAL)).unwrap(),
        )
        .ignore("https://github.com/ChainSafe/forest/issues/4490"),
    ]
}

fn net_tests() -> Vec<RpcTest> {
    // Tests with a known peer id tend to be flaky, use a random peer id to test the unhappy path only
    let random_peer_id = libp2p::PeerId::random().to_string();

    // More net commands should be tested. Tracking issue:
    // https://github.com/ChainSafe/forest/issues/3639
    vec![
        RpcTest::basic(NetAddrsListen::request(()).unwrap()),
        RpcTest::basic(NetPeers::request(()).unwrap()),
        RpcTest::identity(NetListening::request(()).unwrap()),
        RpcTest::basic(NetAgentVersion::request((random_peer_id.clone(),)).unwrap())
            .policy_on_rejected(PolicyOnRejected::PassWithIdenticalError),
        RpcTest::basic(NetFindPeer::request((random_peer_id,)).unwrap())
            .policy_on_rejected(PolicyOnRejected::Pass)
            .ignore("It times out in lotus when peer not found"),
        RpcTest::basic(NetInfo::request(()).unwrap())
            .ignore("Not implemented in Lotus. Why do we even have this method?"),
        RpcTest::basic(NetAutoNatStatus::request(()).unwrap()),
        RpcTest::identity(NetVersion::request(()).unwrap()),
    ]
}

fn node_tests() -> Vec<RpcTest> {
    vec![
        // This is a v1 RPC call. We don't support any v1 calls yet. Tracking
        // issue: https://github.com/ChainSafe/forest/issues/3640
        //RpcTest::basic(ApiInfo::node_status_req())
    ]
}

fn state_tests() -> Vec<RpcTest> {
    vec![
        RpcTest::identity(StateGetBeaconEntry::request((0.into(),)).unwrap()),
        RpcTest::identity(StateGetBeaconEntry::request((1.into(),)).unwrap()),
    ]
}

fn miner_tests_with_tipset<DB: Blockstore>(
    store: &Arc<DB>,
    tipset: &Tipset,
    miner_address: Option<Address>,
) -> anyhow::Result<Vec<RpcTest>> {
    // If no miner address is provided, we can't run any miner tests.
    let Some(miner_address) = miner_address else {
        return Ok(vec![]);
    };

    let mut tests = Vec::new();
    for block in tipset.block_headers() {
        let (bls_messages, secp_messages) = crate::chain::store::block_messages(store, block)?;
        tests.push(miner_create_block_test(
            miner_address,
            tipset,
            bls_messages,
            secp_messages,
        ));
    }
    tests.push(miner_create_block_no_messages_test(miner_address, tipset));
    Ok(tests)
}

fn miner_create_block_test(
    miner: Address,
    tipset: &Tipset,
    bls_messages: Vec<Message>,
    secp_messages: Vec<SignedMessage>,
) -> RpcTest {
    // randomly sign BLS messages so we can test the BLS signature aggregation
    let priv_key = bls_signatures::PrivateKey::generate(&mut rand::thread_rng());
    let signed_bls_msgs = bls_messages
        .into_iter()
        .map(|message| {
            let sig = priv_key.sign(message.cid().to_bytes());
            SignedMessage {
                message,
                signature: Signature::new_bls(sig.as_bytes().to_vec()),
            }
        })
        .collect_vec();

    let block_template = BlockTemplate {
        miner,
        parents: tipset.parents().to_owned(),
        ticket: Ticket::default(),
        eproof: ElectionProof::default(),
        beacon_values: tipset.block_headers().first().beacon_entries.to_owned(),
        messages: [signed_bls_msgs, secp_messages].concat(),
        epoch: tipset.epoch(),
        timestamp: tipset.min_timestamp(),
        winning_post_proof: Vec::default(),
    };
    RpcTest::identity(MinerCreateBlock::request((block_template,)).unwrap())
}

fn miner_create_block_no_messages_test(miner: Address, tipset: &Tipset) -> RpcTest {
    let block_template = BlockTemplate {
        miner,
        parents: tipset.parents().to_owned(),
        ticket: Ticket::default(),
        eproof: ElectionProof::default(),
        beacon_values: tipset.block_headers().first().beacon_entries.to_owned(),
        messages: Vec::default(),
        epoch: tipset.epoch(),
        timestamp: tipset.min_timestamp(),
        winning_post_proof: Vec::default(),
    };
    RpcTest::identity(MinerCreateBlock::request((block_template,)).unwrap())
}

fn state_tests_with_tipset<DB: Blockstore>(
    store: &Arc<DB>,
    tipset: &Tipset,
) -> anyhow::Result<Vec<RpcTest>> {
    let mut tests = vec![
        RpcTest::identity(StateNetworkName::request(())?),
        RpcTest::identity(StateGetNetworkParams::request(())?),
        RpcTest::identity(StateGetActor::request((
            Address::SYSTEM_ACTOR,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateGetRandomnessFromTickets::request((
            DomainSeparationTag::ElectionProofProduction as i64,
            tipset.epoch(),
            "dead beef".as_bytes().to_vec(),
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateGetRandomnessDigestFromTickets::request((
            tipset.epoch(),
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateGetRandomnessFromBeacon::request((
            DomainSeparationTag::ElectionProofProduction as i64,
            tipset.epoch(),
            "dead beef".as_bytes().to_vec(),
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateGetRandomnessDigestFromBeacon::request((
            tipset.epoch(),
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateReadState::request((
            Address::SYSTEM_ACTOR,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateReadState::request((
            Address::SYSTEM_ACTOR,
            Default::default(),
        ))?),
        // This should return `Address::new_id(0xdeadbeef)`
        RpcTest::identity(StateLookupID::request((
            Address::new_id(0xdeadbeef),
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateVerifiedRegistryRootKey::request((tipset
            .key()
            .into(),))?),
        RpcTest::identity(StateVerifierStatus::request((
            Address::VERIFIED_REGISTRY_ACTOR,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateNetworkVersion::request((tipset.key().into(),))?),
        RpcTest::identity(StateListMiners::request((tipset.key().into(),))?),
        RpcTest::identity(StateListActors::request((tipset.key().into(),))?),
        RpcTest::identity(MsigGetAvailableBalance::request((
            Address::new_id(18101), // msig address id
            tipset.key().into(),
        ))?),
        RpcTest::identity(MsigGetPending::request((
            Address::new_id(18101), // msig address id
            tipset.key().into(),
        ))?),
        RpcTest::identity(MsigGetVested::request((
            Address::new_id(18101), // msig address id
            tipset.parents().into(),
            tipset.key().into(),
        ))?),
        RpcTest::identity(MsigGetVestingSchedule::request((
            Address::new_id(18101), // msig address id
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateGetBeaconEntry::request((tipset.epoch(),))?),
        // Not easily verifiable by using addresses extracted from blocks as most of those yield `null`
        // for both Lotus and Forest. Therefore the actor addresses are hardcoded to values that allow
        // for API compatibility verification.
        RpcTest::identity(StateVerifiedClientStatus::request((
            Address::VERIFIED_REGISTRY_ACTOR,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateVerifiedClientStatus::request((
            Address::DATACAP_TOKEN_ACTOR,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateDealProviderCollateralBounds::request((
            1,
            true,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateCirculatingSupply::request((tipset.key().into(),))?),
        RpcTest::identity(StateVMCirculatingSupplyInternal::request((tipset
            .key()
            .into(),))?),
        RpcTest::identity(StateMarketParticipants::request((tipset.key().into(),))?),
        RpcTest::identity(StateMarketDeals::request((tipset.key().into(),))?),
        RpcTest::identity(StateSectorPreCommitInfo::request((
            Default::default(), // invalid address
            u16::MAX as _,
            tipset.key().into(),
        ))?)
        .policy_on_rejected(PolicyOnRejected::Pass),
        RpcTest::identity(StateSectorGetInfo::request((
            Default::default(), // invalid address
            u16::MAX as _,
            tipset.key().into(),
        ))?)
        .policy_on_rejected(PolicyOnRejected::Pass),
        RpcTest::identity(StateGetAllocationIdForPendingDeal::request((
            u16::MAX as _, // Invalid deal id
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateGetAllocationForPendingDeal::request((
            u16::MAX as _, // Invalid deal id
            tipset.key().into(),
        ))?),
    ];

    for &pending_deal_id in
        StateGetAllocationIdForPendingDeal::get_allocations_for_pending_deals(store, tipset)?
            .keys()
            .take(COLLECTION_SAMPLE_SIZE)
    {
        tests.extend([
            RpcTest::identity(StateGetAllocationIdForPendingDeal::request((
                pending_deal_id,
                tipset.key().into(),
            ))?),
            RpcTest::identity(StateGetAllocationForPendingDeal::request((
                pending_deal_id,
                tipset.key().into(),
            ))?),
        ]);
    }

    // Get deals
    let (deals, deals_map) = {
        let state = StateTree::new_from_root(store.clone(), tipset.parent_state())?;
        let actor = state.get_required_actor(&Address::MARKET_ACTOR)?;
        let market_state = market::State::load(&store, actor.code, actor.state)?;
        let proposals = market_state.proposals(&store)?;
        let mut deals = vec![];
        let mut deals_map = HashMap::default();
        proposals.for_each(|deal_id, deal_proposal| {
            deals.push(deal_id);
            deals_map.insert(deal_id, deal_proposal);
            Ok(())
        })?;
        (deals, deals_map)
    };

    // Take 5 deals from each tipset
    for deal in deals.into_iter().take(COLLECTION_SAMPLE_SIZE) {
        tests.push(RpcTest::identity(StateMarketStorageDeal::request((
            deal,
            tipset.key().into(),
        ))?));
    }

    for block in tipset.block_headers() {
        tests.extend([
            RpcTest::identity(StateMinerAllocated::request((
                block.miner_address,
                tipset.key().into(),
            ))?),
            RpcTest::identity(StateMinerActiveSectors::request((
                block.miner_address,
                tipset.key().into(),
            ))?),
            RpcTest::identity(StateLookupID::request((
                block.miner_address,
                tipset.key().into(),
            ))?),
            RpcTest::identity(StateLookupRobustAddress::request((
                block.miner_address,
                tipset.key().into(),
            ))?),
            RpcTest::identity(StateMinerSectors::request((
                block.miner_address,
                None,
                tipset.key().into(),
            ))?),
            RpcTest::identity(StateMinerPartitions::request((
                block.miner_address,
                0,
                tipset.key().into(),
            ))?),
            RpcTest::identity(StateMarketBalance::request((
                block.miner_address,
                tipset.key().into(),
            ))?),
            RpcTest::identity(StateMinerInfo::request((
                block.miner_address,
                tipset.key().into(),
            ))?),
            RpcTest::identity(StateMinerPower::request((
                block.miner_address,
                tipset.key().into(),
            ))?),
            RpcTest::identity(StateMinerDeadlines::request((
                block.miner_address,
                tipset.key().into(),
            ))?),
            RpcTest::identity(StateMinerProvingDeadline::request((
                block.miner_address,
                tipset.key().into(),
            ))?),
            RpcTest::identity(StateMinerAvailableBalance::request((
                block.miner_address,
                tipset.key().into(),
            ))?),
            RpcTest::identity(StateMinerFaults::request((
                block.miner_address,
                tipset.key().into(),
            ))?),
            RpcTest::identity(MinerGetBaseInfo::request((
                block.miner_address,
                block.epoch,
                tipset.key().into(),
            ))?),
            RpcTest::identity(StateMinerRecoveries::request((
                block.miner_address,
                tipset.key().into(),
            ))?),
            RpcTest::identity(StateMinerSectorCount::request((
                block.miner_address,
                tipset.key().into(),
            ))?),
            RpcTest::identity(StateGetClaims::request((
                block.miner_address,
                tipset.key().into(),
            ))?),
            RpcTest::identity(StateGetAllClaims::request((tipset.key().into(),))?),
            RpcTest::identity(StateGetAllAllocations::request((tipset.key().into(),))?),
            RpcTest::identity(StateSectorPreCommitInfo::request((
                block.miner_address,
                u16::MAX as _, // invalid sector number
                tipset.key().into(),
            ))?)
            .policy_on_rejected(PolicyOnRejected::PassWithIdenticalError),
            RpcTest::identity(StateSectorGetInfo::request((
                block.miner_address,
                u16::MAX as _, // invalid sector number
                tipset.key().into(),
            ))?)
            .policy_on_rejected(PolicyOnRejected::PassWithIdenticalError),
        ]);
        for claim_id in StateGetClaims::get_claims(store, &block.miner_address, tipset)?
            .keys()
            .take(COLLECTION_SAMPLE_SIZE)
        {
            tests.extend([RpcTest::identity(StateGetClaim::request((
                block.miner_address,
                *claim_id,
                tipset.key().into(),
            ))?)]);
        }
        for address in StateGetAllocations::get_valid_actor_addresses(store, tipset)?
            .take(COLLECTION_SAMPLE_SIZE)
        {
            tests.extend([RpcTest::identity(StateGetAllocations::request((
                address,
                tipset.key().into(),
            ))?)]);
            for allocation_id in StateGetAllocations::get_allocations(store, &address, tipset)?
                .keys()
                .take(COLLECTION_SAMPLE_SIZE)
            {
                tests.extend([RpcTest::identity(StateGetAllocation::request((
                    address,
                    *allocation_id,
                    tipset.key().into(),
                ))?)]);
            }
        }
        for sector in StateSectorGetInfo::get_sectors(store, &block.miner_address, tipset)?
            .into_iter()
            .take(COLLECTION_SAMPLE_SIZE)
        {
            tests.extend([
                RpcTest::identity(StateSectorGetInfo::request((
                    block.miner_address,
                    sector,
                    tipset.key().into(),
                ))?),
                RpcTest::identity(StateMinerSectors::request((
                    block.miner_address,
                    {
                        let mut bf = BitField::new();
                        bf.set(sector);
                        Some(bf)
                    },
                    tipset.key().into(),
                ))?),
                RpcTest::identity(StateSectorExpiration::request((
                    block.miner_address,
                    sector,
                    tipset.key().into(),
                ))?)
                .policy_on_rejected(PolicyOnRejected::PassWithIdenticalError),
                RpcTest::identity(StateSectorPartition::request((
                    block.miner_address,
                    sector,
                    tipset.key().into(),
                ))?),
                RpcTest::identity(StateMinerSectorAllocated::request((
                    block.miner_address,
                    sector,
                    tipset.key().into(),
                ))?),
            ]);
        }
        for sector in StateSectorPreCommitInfo::get_sectors(store, &block.miner_address, tipset)?
            .into_iter()
            .take(COLLECTION_SAMPLE_SIZE)
        {
            tests.extend([RpcTest::identity(StateSectorPreCommitInfo::request((
                block.miner_address,
                sector,
                tipset.key().into(),
            ))?)]);
        }
        for info in StateSectorPreCommitInfo::get_sector_pre_commit_infos(
            store,
            &block.miner_address,
            tipset,
        )?
        .into_iter()
        .take(COLLECTION_SAMPLE_SIZE)
        .filter(|info| {
            !info.deal_ids.iter().any(|id| {
                if let Some(Ok(deal)) = deals_map.get(id) {
                    tipset.epoch() > deal.start_epoch || info.expiration > deal.end_epoch
                } else {
                    true
                }
            })
        }) {
            tests.extend([RpcTest::identity(
                StateMinerInitialPledgeCollateral::request((
                    block.miner_address,
                    info.clone(),
                    tipset.key().into(),
                ))?,
            )]);
            tests.extend([RpcTest::identity(
                StateMinerPreCommitDepositForPower::request((
                    block.miner_address,
                    info,
                    tipset.key().into(),
                ))?,
            )]);
        }

        let (bls_messages, secp_messages) = crate::chain::store::block_messages(store, block)?;
        for msg_cid in sample_message_cids(bls_messages.iter(), secp_messages.iter()) {
            tests.extend([
                RpcTest::identity(StateReplay::request((tipset.key().into(), msg_cid))?),
                validate_message_lookup(
                    StateWaitMsg::request((msg_cid, 0, 10101, true))?
                        .with_timeout(Duration::from_secs(15)),
                ),
                validate_message_lookup(
                    StateWaitMsg::request((msg_cid, 0, 10101, false))?
                        .with_timeout(Duration::from_secs(15)),
                ),
                validate_message_lookup(StateSearchMsg::request((msg_cid,))?),
                validate_message_lookup(StateSearchMsgLimited::request((msg_cid, 800))?),
            ]);
        }
        for msg in sample_messages(bls_messages.iter(), secp_messages.iter()) {
            tests.extend([
                RpcTest::identity(StateAccountKey::request((msg.from(), tipset.key().into()))?),
                RpcTest::identity(StateAccountKey::request((msg.from(), Default::default()))?),
                RpcTest::identity(StateLookupID::request((msg.from(), tipset.key().into()))?),
                RpcTest::identity(StateListMessages::request((
                    MessageFilter {
                        from: Some(msg.from()),
                        to: Some(msg.to()),
                    },
                    tipset.key().into(),
                    tipset.epoch(),
                ))?),
                RpcTest::identity(StateListMessages::request((
                    MessageFilter {
                        from: Some(msg.from()),
                        to: None,
                    },
                    tipset.key().into(),
                    tipset.epoch(),
                ))?),
                RpcTest::identity(StateListMessages::request((
                    MessageFilter {
                        from: None,
                        to: Some(msg.to()),
                    },
                    tipset.key().into(),
                    tipset.epoch(),
                ))?),
                RpcTest::identity(StateCall::request((msg.clone(), tipset.key().into()))?),
            ]);
        }
    }

    Ok(tests)
}

fn wallet_tests(worker_address: Option<Address>) -> Vec<RpcTest> {
    // This address has been funded by the calibnet faucet and the private keys
    // has been discarded. It should always have a non-zero balance.
    let known_wallet = Address::from_str("t1c4dkec3qhrnrsa4mccy7qntkyq2hhsma4sq7lui").unwrap();
    // "Hello world!" signed with the above address:
    let signature = "44364ca78d85e53dda5ac6f719a4f2de3261c17f58558ab7730f80c478e6d43775244e7d6855afad82e4a1fd6449490acfa88e3fcfe7c1fe96ed549c100900b400";
    let text = "Hello world!".as_bytes().to_vec();
    let sig_bytes = hex::decode(signature).unwrap();
    let signature = match known_wallet.protocol() {
        Protocol::Secp256k1 => Signature::new_secp256k1(sig_bytes),
        Protocol::BLS => Signature::new_bls(sig_bytes),
        _ => panic!("Invalid signature (must be bls or secp256k1)"),
    };

    let mut tests = vec![
        RpcTest::identity(WalletBalance::request((known_wallet,)).unwrap()),
        RpcTest::identity(WalletValidateAddress::request((known_wallet.to_string(),)).unwrap()),
        RpcTest::identity(WalletVerify::request((known_wallet, text, signature)).unwrap()),
    ];

    // If a worker address is provided, we can test wallet methods requiring
    // a shared key.
    if let Some(worker_address) = worker_address {
        use base64::{prelude::BASE64_STANDARD, Engine};
        let msg =
            BASE64_STANDARD.encode("Ph'nglui mglw'nafh Cthulhu R'lyeh wgah'nagl fhtagn".as_bytes());
        tests.push(RpcTest::identity(
            WalletSign::request((worker_address, msg.into())).unwrap(),
        ));
        tests.push(RpcTest::identity(
            WalletSign::request((worker_address, Vec::new())).unwrap(),
        ));
        let msg: Message = Message {
            from: worker_address,
            to: worker_address,
            value: TokenAmount::from_whole(1),
            method_num: METHOD_SEND,
            ..Default::default()
        };
        tests.push(RpcTest::identity(
            WalletSignMessage::request((worker_address, msg)).unwrap(),
        ));
    }
    tests
}

fn eth_tests() -> Vec<RpcTest> {
    let mut tests = vec![];
    for use_alias in [false, true] {
        tests.push(RpcTest::identity(
            EthAccounts::request_with_alias((), use_alias).unwrap(),
        ));
        tests.push(RpcTest::basic(
            EthBlockNumber::request_with_alias((), use_alias).unwrap(),
        ));
        tests.push(RpcTest::identity(
            EthChainId::request_with_alias((), use_alias).unwrap(),
        ));
        // There is randomness in the result of this API
        tests.push(RpcTest::basic(
            EthGasPrice::request_with_alias((), use_alias).unwrap(),
        ));
        tests.push(RpcTest::basic(
            EthSyncing::request_with_alias((), use_alias).unwrap(),
        ));
        tests.push(RpcTest::identity(
            EthGetBalance::request_with_alias(
                (
                    EthAddress::from_str("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa").unwrap(),
                    BlockNumberOrHash::from_predefined(Predefined::Latest),
                ),
                use_alias,
            )
            .unwrap(),
        ));
        tests.push(RpcTest::identity(
            EthGetBalance::request_with_alias(
                (
                    EthAddress::from_str("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa").unwrap(),
                    BlockNumberOrHash::from_predefined(Predefined::Pending),
                ),
                use_alias,
            )
            .unwrap(),
        ));
        tests.push(RpcTest::basic(
            Web3ClientVersion::request_with_alias((), use_alias).unwrap(),
        ));
        tests.push(RpcTest::basic(
            EthMaxPriorityFeePerGas::request_with_alias((), use_alias).unwrap(),
        ));
        tests.push(RpcTest::identity(
            EthProtocolVersion::request_with_alias((), use_alias).unwrap(),
        ));
    }
    tests
}

fn eth_tests_with_tipset<DB: Blockstore>(store: &Arc<DB>, shared_tipset: &Tipset) -> Vec<RpcTest> {
    let block_cid = shared_tipset.key().cid().unwrap();
    let block_hash: Hash = block_cid.into();

    let mut tests = vec![
        RpcTest::identity(
            EthGetBalance::request((
                EthAddress::from_str("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa").unwrap(),
                BlockNumberOrHash::from_block_number(shared_tipset.epoch()),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBalance::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                BlockNumberOrHash::from_block_number(shared_tipset.epoch()),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBalance::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                BlockNumberOrHash::from_block_number_object(shared_tipset.epoch()),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBalance::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                BlockNumberOrHash::from_block_hash_object(block_hash.clone(), false),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBalance::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                BlockNumberOrHash::from_block_hash_object(block_hash.clone(), true),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBlockByNumber::request((
                BlockNumberOrHash::from_block_number(shared_tipset.epoch()),
                false,
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBlockByNumber::request((
                BlockNumberOrHash::from_block_number(shared_tipset.epoch()),
                true,
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBlockTransactionCountByHash::request((block_hash.clone(),)).unwrap(),
        ),
        RpcTest::identity(
            EthGetBlockTransactionCountByNumber::request((Int64(shared_tipset.epoch()),)).unwrap(),
        ),
        RpcTest::identity(
            EthGetTransactionCount::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                BlockNumberOrHash::from_block_hash_object(block_hash.clone(), true),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetStorageAt::request((
                // https://filfox.info/en/address/f410fpoidg73f7krlfohnla52dotowde5p2sejxnd4mq
                EthAddress::from_str("0x7B90337f65fAA2B2B8ed583ba1Ba6EB0C9D7eA44").unwrap(),
                EthBytes(vec![0xa]),
                BlockNumberOrHash::BlockNumber(Int64(shared_tipset.epoch())),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthFeeHistory::request((
                10.into(),
                BlockNumberOrPredefined::BlockNumber(shared_tipset.epoch().into()),
                None,
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthFeeHistory::request((
                10.into(),
                BlockNumberOrPredefined::BlockNumber(shared_tipset.epoch().into()),
                Some(vec![10., 50., 90.]),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetCode::request((
                // https://filfox.info/en/address/f410fpoidg73f7krlfohnla52dotowde5p2sejxnd4mq
                EthAddress::from_str("0x7B90337f65fAA2B2B8ed583ba1Ba6EB0C9D7eA44").unwrap(),
                BlockNumberOrHash::from_block_number(shared_tipset.epoch()),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetCode::request((
                // https://filfox.info/en/address/f410fpoidg73f7krlfohnla52dotowde5p2sejxnd4mq
                Address::from_str("f410fpoidg73f7krlfohnla52dotowde5p2sejxnd4mq")
                    .unwrap()
                    .try_into()
                    .unwrap(),
                BlockNumberOrHash::from_block_number(shared_tipset.epoch()),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBlockByHash::request((
                BlockNumberOrHash::from_block_hash(block_hash.clone()),
                false,
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBlockByHash::request((
                BlockNumberOrHash::from_block_hash(block_hash.clone()),
                true,
            ))
            .unwrap(),
        ),
        RpcTest::identity(EthGetTransactionHashByCid::request((block_cid,)).unwrap()),
    ];

    for block in shared_tipset.block_headers() {
        let (bls_messages, secp_messages) =
            crate::chain::store::block_messages(store, block).unwrap();
        for msg in sample_messages(bls_messages.iter(), secp_messages.iter()) {
            if let Ok(eth_to_addr) = msg.to.try_into() {
                tests.extend([RpcTest::identity(
                    EthEstimateGas::request((
                        EthCallMessage {
                            from: None,
                            to: Some(eth_to_addr),
                            value: msg.value.clone().into(),
                            data: msg.params.clone().into(),
                            ..Default::default()
                        },
                        Some(BlockNumberOrHash::BlockNumber(shared_tipset.epoch().into())),
                    ))
                    .unwrap(),
                )
                .policy_on_rejected(PolicyOnRejected::Pass)]);
            }
        }
    }

    tests
}

fn eth_state_tests_with_tipset<DB: Blockstore>(
    store: &Arc<DB>,
    shared_tipset: &Tipset,
    eth_chain_id: EthChainIdType,
) -> anyhow::Result<Vec<RpcTest>> {
    let mut tests = vec![];

    for block in shared_tipset.block_headers() {
        let state = StateTree::new_from_root(store.clone(), shared_tipset.parent_state())?;

        let (bls_messages, secp_messages) = crate::chain::store::block_messages(store, block)?;
        for smsg in sample_signed_messages(bls_messages.iter(), secp_messages.iter()) {
            let tx = new_eth_tx_from_signed_message(&smsg, &state, eth_chain_id)?;
            tests.push(RpcTest::identity(
                EthGetMessageCidByTransactionHash::request((tx.hash,))?,
            ));
        }
    }
    tests.push(RpcTest::identity(
        EthGetMessageCidByTransactionHash::request((Hash::from_str(
            "0x37690cfec6c1bf4c3b9288c7a5d783e98731e90b0a4c177c2a374c7a9427355f",
        )?,))?,
    ));

    Ok(tests)
}

fn gas_tests_with_tipset(shared_tipset: &Tipset) -> Vec<RpcTest> {
    // This is a testnet address with a few FILs. The private key has been
    // discarded. If calibnet is reset, a new address should be created.
    let addr = Address::from_str("t15ydyu3d65gznpp2qxwpkjsgz4waubeunn6upvla").unwrap();
    let message = Message {
        from: addr,
        to: addr,
        value: TokenAmount::from_whole(1),
        method_num: METHOD_SEND,
        ..Default::default()
    };

    // The tipset is only used for resolving the 'from' address and not when
    // computing the gas cost. This means that the `GasEstimateGasLimit` method
    // is inherently non-deterministic but I'm fairly sure we're compensated for
    // everything. If not, this test will be flaky. Instead of disabling it, we
    // should relax the verification requirement.
    vec![RpcTest::identity(
        GasEstimateGasLimit::request((message, shared_tipset.key().into())).unwrap(),
    )]
}

// Extract tests that use chain-specific data such as block CIDs or message
// CIDs. Right now, only the last `n_tipsets` tipsets are used.
fn snapshot_tests(
    store: Arc<ManyCar>,
    num_tipsets: usize,
    miner_address: Option<Address>,
    eth_chain_id: u64,
) -> anyhow::Result<Vec<RpcTest>> {
    let mut tests = vec![];
    // shared_tipset in the snapshot might not be finalized for the offline RPC server
    // use heaviest - 10 instead
    let shared_tipset = store
        .heaviest_tipset()?
        .chain(&store)
        .take(10)
        .last()
        .expect("Infallible");

    for tipset in shared_tipset.chain(&store).take(num_tipsets) {
        tests.extend(chain_tests_with_tipset(&store, &tipset)?);
        tests.extend(miner_tests_with_tipset(&store, &tipset, miner_address)?);
        tests.extend(state_tests_with_tipset(&store, &tipset)?);
        tests.extend(eth_tests_with_tipset(&store, &tipset));
        tests.extend(gas_tests_with_tipset(&tipset));
        tests.extend(mpool_tests_with_tipset(&tipset));
        tests.extend(eth_state_tests_with_tipset(&store, &tipset, eth_chain_id)?);
    }
    Ok(tests)
}

fn sample_message_cids<'a>(
    bls_messages: impl Iterator<Item = &'a Message> + 'a,
    secp_messages: impl Iterator<Item = &'a SignedMessage> + 'a,
) -> impl Iterator<Item = Cid> + 'a {
    bls_messages
        .map(|m| m.cid())
        .unique()
        .take(COLLECTION_SAMPLE_SIZE)
        .chain(
            secp_messages
                .map(|m| m.cid())
                .unique()
                .take(COLLECTION_SAMPLE_SIZE),
        )
        .unique()
}

fn sample_messages<'a>(
    bls_messages: impl Iterator<Item = &'a Message> + 'a,
    secp_messages: impl Iterator<Item = &'a SignedMessage> + 'a,
) -> impl Iterator<Item = &'a Message> + 'a {
    bls_messages
        .unique()
        .take(COLLECTION_SAMPLE_SIZE)
        .chain(
            secp_messages
                .map(SignedMessage::message)
                .unique()
                .take(COLLECTION_SAMPLE_SIZE),
        )
        .unique()
}

fn sample_signed_messages<'a>(
    bls_messages: impl Iterator<Item = &'a Message> + 'a,
    secp_messages: impl Iterator<Item = &'a SignedMessage> + 'a,
) -> impl Iterator<Item = SignedMessage> + 'a {
    bls_messages
        .unique()
        .take(COLLECTION_SAMPLE_SIZE)
        .map(|msg| {
            let sig = Signature::new_bls(vec![]);
            SignedMessage::new_unchecked(msg.clone(), sig)
        })
        .chain(secp_messages.cloned().unique().take(COLLECTION_SAMPLE_SIZE))
        .unique()
}

fn create_tests(
    CreateTestsArgs {
        n_tipsets,
        miner_address,
        worker_address,
        eth_chain_id,
        snapshot_files,
    }: CreateTestsArgs,
) -> anyhow::Result<Vec<RpcTest>> {
    let mut tests = vec![];
    tests.extend(common_tests());
    tests.extend(beacon_tests());
    tests.extend(chain_tests());
    tests.extend(mpool_tests());
    tests.extend(net_tests());
    tests.extend(node_tests());
    tests.extend(wallet_tests(worker_address));
    tests.extend(eth_tests());
    tests.extend(state_tests());
    if !snapshot_files.is_empty() {
        let store = Arc::new(ManyCar::try_from(snapshot_files)?);
        tests.extend(snapshot_tests(
            store,
            n_tipsets,
            miner_address,
            eth_chain_id,
        )?);
    }
    tests.sort_by_key(|test| test.request.method_name);
    Ok(tests)
}

async fn start_offline_server(
    snapshot_files: Vec<PathBuf>,
    chain: NetworkChain,
    rpc_port: u16,
    auto_download_snapshot: bool,
    height: i64,
) -> anyhow::Result<()> {
    info!("Configuring Offline RPC Server");
    let db = Arc::new(ManyCar::new(MemoryDB::default()));

    let snapshot_files = if snapshot_files.is_empty() {
        let (snapshot_url, num_bytes, path) =
            crate::cli_shared::snapshot::peek(TrustedVendor::default(), &chain)
                .await
                .context("couldn't get snapshot size")?;
        if !auto_download_snapshot {
            warn!("Automatic snapshot download is disabled.");
            let message = format!(
                "Fetch a {} snapshot to the current directory? (denying will exit the program). ",
                indicatif::HumanBytes(num_bytes)
            );
            let have_permission =
                dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
                    .with_prompt(message)
                    .default(false)
                    .interact()
                    .unwrap_or(false);
            if !have_permission {
                anyhow::bail!("No snapshot provided, exiting offline RPC setup.");
            }
        }
        info!(
            "Downloading latest snapshot for {} size {}",
            chain,
            indicatif::HumanBytes(num_bytes)
        );
        let downloaded_snapshot_path = std::env::current_dir()?.join(path);
        download_to(&snapshot_url, &downloaded_snapshot_path).await?;
        info!("Snapshot downloaded");
        vec![downloaded_snapshot_path]
    } else {
        snapshot_files
    };
    db.read_only_files(snapshot_files.iter().cloned())?;
    info!("Using chain config for {chain}");
    let chain_config = Arc::new(ChainConfig::from_chain(&chain));
    if chain_config.is_testnet() {
        CurrentNetwork::set_global(Network::Testnet);
    }
    let sync_config = Arc::new(SyncConfig::default());
    let genesis_header =
        read_genesis_header(None, chain_config.genesis_bytes(&db).await?.as_deref(), &db).await?;
    let chain_store = Arc::new(ChainStore::new(
        db.clone(),
        db.clone(),
        db.clone(),
        chain_config.clone(),
        genesis_header.clone(),
    )?);
    let state_manager = Arc::new(StateManager::new(
        chain_store.clone(),
        chain_config,
        sync_config,
    )?);
    let head_ts = Arc::new(db.heaviest_tipset()?);

    state_manager
        .chain_store()
        .set_heaviest_tipset(head_ts.clone())?;

    populate_eth_mappings(&state_manager, &head_ts)?;

    let (network_send, _) = flume::bounded(5);
    let (tipset_send, _) = flume::bounded(5);
    let network_name = get_network_name_from_genesis(&genesis_header, &state_manager)?;
    let message_pool = MessagePool::new(
        MpoolRpcProvider::new(chain_store.publisher().clone(), state_manager.clone()),
        network_name.clone(),
        network_send.clone(),
        Default::default(),
        state_manager.chain_config().clone(),
        &mut JoinSet::new(),
    )?;

    // Validate tipsets since the {height} EPOCH when `height >= 0`,
    // or valiadte the last {-height} EPOCH(s) when `height < 0`
    let n_ts_to_validate = if height > 0 {
        (head_ts.epoch() - height).max(0)
    } else {
        -height
    } as usize;
    if n_ts_to_validate > 0 {
        state_manager.validate_tipsets(head_ts.chain_arc(&db).take(n_ts_to_validate))?;
    }

    let (shutdown, shutdown_recv) = mpsc::channel(1);

    let rpc_state = RPCState {
        state_manager,
        keystore: Arc::new(RwLock::new(KeyStore::new(KeyStoreConfig::Memory)?)),
        mpool: Arc::new(message_pool),
        bad_blocks: Default::default(),
        sync_state: Arc::new(parking_lot::RwLock::new(Default::default())),
        network_send,
        network_name,
        start_time: chrono::Utc::now(),
        shutdown,
        tipset_send,
    };
    rpc_state.sync_state.write().set_stage(SyncStage::Idle);
    start_offline_rpc(rpc_state, rpc_port, shutdown_recv).await?;

    Ok(())
}

async fn start_offline_rpc<DB>(
    state: RPCState<DB>,
    rpc_port: u16,
    mut shutdown_recv: mpsc::Receiver<()>,
) -> anyhow::Result<()>
where
    DB: Blockstore + Send + Sync + 'static,
{
    info!("Starting offline RPC Server");
    let rpc_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), rpc_port);
    let mut terminate = signal(SignalKind::terminate())?;

    let result = tokio::select! {
        ret = start_rpc(state, rpc_address) => ret,
        _ = ctrl_c() => {
            info!("Keyboard interrupt.");
            Ok(())
        },
        _ = terminate.recv() => {
            info!("Received SIGTERM.");
            Ok(())
        },
        _ = shutdown_recv.recv() => {
            info!("Client requested a shutdown.");
            Ok(())
        },
    };
    crate::utils::io::terminal_cleanup();
    result
}

#[allow(clippy::too_many_arguments)]
async fn run_tests(
    tests: impl IntoIterator<Item = RpcTest>,
    forest: impl Into<Arc<rpc::Client>>,
    lotus: impl Into<Arc<rpc::Client>>,
    max_concurrent_requests: usize,
    filter_file: Option<PathBuf>,
    filter: String,
    run_ignored: RunIgnored,
    fail_fast: bool,
) -> anyhow::Result<()> {
    let forest = Into::<Arc<rpc::Client>>::into(forest);
    let lotus = Into::<Arc<rpc::Client>>::into(lotus);
    let semaphore = Arc::new(Semaphore::new(max_concurrent_requests));
    let mut futures = FuturesUnordered::new();

    let filter_list = if let Some(filter_file) = &filter_file {
        FilterList::new_from_file(filter_file)?
    } else {
        FilterList::default().allow(filter.clone())
    };

    // deduplicate tests by their hash-able representations
    for test in tests.into_iter().unique_by(
        |RpcTest {
             request:
                 rpc::Request {
                     method_name,
                     params,
                     api_paths,
                     ..
                 },
             ignore,
             ..
         }| (*method_name, params.clone(), *api_paths, ignore.is_some()),
    ) {
        // By default, do not run ignored tests.
        if matches!(run_ignored, RunIgnored::Default) && test.ignore.is_some() {
            continue;
        }
        // If in `IgnoreOnly` mode, only run ignored tests.
        if matches!(run_ignored, RunIgnored::IgnoredOnly) && test.ignore.is_none() {
            continue;
        }

        if !filter_list.authorize(test.request.method_name) {
            continue;
        }

        // Acquire a permit from the semaphore before spawning a test
        let permit = semaphore.clone().acquire_owned().await?;
        let forest = forest.clone();
        let lotus = lotus.clone();
        let future = tokio::spawn(async move {
            let test_result = test.run(&forest, &lotus).await;
            drop(permit); // Release the permit after test execution
            (test, test_result)
        });

        futures.push(future);
    }

    let mut success_results = HashMap::default();
    let mut failed_results = HashMap::default();
    let mut fail_details = Vec::new();
    while let Some(Ok((test, test_result))) = futures.next().await {
        let method_name = test.request.method_name;
        let forest_status = test_result.forest_status;
        let lotus_status = test_result.lotus_status;
        let success = match (&forest_status, &lotus_status) {
            (TestSummary::Valid, TestSummary::Valid)
            | (TestSummary::Timeout, TestSummary::Timeout) => true,
            (TestSummary::Rejected(ref reason_forest), TestSummary::Rejected(ref reason_lotus)) => {
                match test.policy_on_rejected {
                    PolicyOnRejected::Pass => true,
                    PolicyOnRejected::PassWithIdenticalError if reason_forest == reason_lotus => {
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        };
        let result_entry = (method_name, forest_status, lotus_status);
        if success {
            success_results
                .entry(result_entry)
                .and_modify(|v| *v += 1)
                .or_insert(1u32);
        } else {
            if let Some(test_result) = test_result.test_dump {
                fail_details.push(test_result);
            }
            failed_results
                .entry(result_entry)
                .and_modify(|v| *v += 1)
                .or_insert(1u32);
        }

        if !failed_results.is_empty() && fail_fast {
            break;
        }
    }
    print_error_details(&fail_details);
    print_test_results(&success_results, &failed_results);

    if failed_results.is_empty() {
        Ok(())
    } else {
        Err(anyhow::Error::msg("Some tests failed"))
    }
}

fn print_error_details(fail_details: &[TestDump]) {
    for dump in fail_details {
        println!("{dump}")
    }
}

fn print_test_results(
    success_results: &HashMap<(&'static str, TestSummary, TestSummary), u32>,
    failed_results: &HashMap<(&'static str, TestSummary, TestSummary), u32>,
) {
    // Combine all results
    let mut combined_results = success_results.clone();
    for (key, &value) in failed_results {
        combined_results.insert(key.clone(), value);
    }

    // Collect and display results in Markdown format
    let mut results = combined_results.into_iter().collect::<Vec<_>>();
    results.sort();
    println!("{}", format_as_markdown(&results));
}

fn format_as_markdown(results: &[((&'static str, TestSummary, TestSummary), u32)]) -> String {
    let mut builder = Builder::default();

    builder.push_record(["RPC Method", "Forest", "Lotus"]);

    for ((method, forest_status, lotus_status), n) in results {
        builder.push_record([
            if *n > 1 {
                format!("{} ({})", method, n)
            } else {
                method.to_string()
            },
            format!("{:?}", forest_status),
            format!("{:?}", lotus_status),
        ]);
    }

    builder.build().with(Style::markdown()).to_string()
}

fn validate_message_lookup(req: rpc::Request<MessageLookup>) -> RpcTest {
    use libipld_core::ipld::Ipld;

    RpcTest::validate(req, |mut forest, mut lotus| {
        // TODO(hanabi1224): https://github.com/ChainSafe/forest/issues/3784
        forest.return_dec = Ipld::Null;
        lotus.return_dec = Ipld::Null;
        forest == lotus
    })
}

/// A filter list that allows or rejects RPC methods based on their name.
#[derive(Default)]
struct FilterList {
    allow: Vec<String>,
    reject: Vec<String>,
}

impl FilterList {
    fn new_from_file(file: &Path) -> anyhow::Result<Self> {
        let (allow, reject) = Self::create_allow_reject_list(file)?;
        Ok(Self { allow, reject })
    }

    /// Authorize (or not) an RPC method based on its name.
    /// If the allow list is empty, all methods are authorized, unless they are rejected.
    fn authorize(&self, entry: &str) -> bool {
        (self.allow.is_empty() || self.allow.iter().any(|a| entry.contains(a)))
            && !self.reject.iter().any(|r| entry.contains(r))
    }

    fn allow(mut self, entry: String) -> Self {
        self.allow.push(entry);
        self
    }

    #[allow(dead_code)]
    fn reject(mut self, entry: String) -> Self {
        self.reject.push(entry);
        self
    }

    /// Create a list of allowed and rejected RPC methods from a file.
    fn create_allow_reject_list(file: &Path) -> anyhow::Result<(Vec<String>, Vec<String>)> {
        let filter_file = std::fs::read_to_string(file)?;
        let (reject, allow): (Vec<_>, Vec<_>) = filter_file
            .lines()
            .map(|line| line.trim().to_owned())
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .partition(|line| line.starts_with('!'));

        let reject = reject
            .into_iter()
            .map(|entry| entry.trim_start_matches('!').to_owned())
            .collect::<Vec<_>>();

        Ok((allow, reject))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_filter_list_creation() {
        // Create a temporary file and write some test data to it
        let mut filter_file = tempfile::Builder::new().tempfile().unwrap();
        let list = FilterList::new_from_file(filter_file.path()).unwrap();
        assert!(list.allow.is_empty());
        assert!(list.reject.is_empty());

        write!(
            filter_file,
            r#"# This is a comment
            !cthulhu
            azathoth
            !nyarlathotep
            "#
        )
        .unwrap();

        let list = FilterList::new_from_file(filter_file.path()).unwrap();
        assert_eq!(list.allow, vec!["azathoth".to_string()]);
        assert_eq!(
            list.reject,
            vec!["cthulhu".to_string(), "nyarlathotep".to_string()]
        );

        let list = list
            .allow("shub-niggurath".to_string())
            .reject("yog-sothoth".to_string());
        assert_eq!(
            list.allow,
            vec!["azathoth".to_string(), "shub-niggurath".to_string()]
        );
    }

    #[test]
    fn test_filter_list_authorize() {
        let list = FilterList::default();
        // if allow is empty, all entries are authorized
        assert!(list.authorize("Filecoin.ChainGetBlock"));
        assert!(list.authorize("Filecoin.StateNetworkName"));

        // all entries are authorized, except the rejected ones
        let list = list.reject("Network".to_string());
        assert!(list.authorize("Filecoin.ChainGetBlock"));

        // case-sensitive
        assert!(list.authorize("Filecoin.StatenetworkName"));
        assert!(!list.authorize("Filecoin.StateNetworkName"));

        // if allow is not empty, only the allowed entries are authorized
        let list = FilterList::default().allow("Chain".to_string());
        assert!(list.authorize("Filecoin.ChainGetBlock"));
        assert!(!list.authorize("Filecoin.StateNetworkName"));

        // unless they are rejected
        let list = list.reject("GetBlock".to_string());
        assert!(!list.authorize("Filecoin.ChainGetBlock"));
        assert!(list.authorize("Filecoin.ChainGetMessage"));

        // reject takes precedence over allow
        let list = FilterList::default()
            .allow("Chain".to_string())
            .reject("Chain".to_string());
        assert!(!list.authorize("Filecoin.ChainGetBlock"));
    }
}
