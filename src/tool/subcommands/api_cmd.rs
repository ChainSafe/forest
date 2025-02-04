// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod generate_test_snapshot;
mod test_snapshot;

use crate::blocks::{ElectionProof, Ticket, Tipset};
use crate::db::car::ManyCar;
use crate::eth::{EthChainId as EthChainIdType, SAFE_EPOCH_DELAY};
use crate::lotus_json::HasLotusJson;
use crate::message::{Message as _, SignedMessage};
use crate::networks::NetworkChain;
use crate::rpc;
use crate::rpc::auth::AuthNewParams;
use crate::rpc::beacon::BeaconGetEntry;
use crate::rpc::eth::{
    new_eth_tx_from_signed_message, types::*, BlockNumberOrHash, EthInt64, Predefined,
};
use crate::rpc::gas::GasEstimateGasLimit;
use crate::rpc::miner::BlockTemplate;
use crate::rpc::state::StateGetAllClaims;
use crate::rpc::types::{ApiTipsetKey, MessageFilter, MessageLookup};
use crate::rpc::{prelude::*, Permission};
use crate::shim::actors::market;
use crate::shim::actors::MarketActorStateLoad as _;
use crate::shim::sector::SectorSize;
use crate::shim::{
    address::{Address, Protocol},
    crypto::Signature,
    econ::TokenAmount,
    message::{Message, METHOD_SEND},
    state_tree::StateTree,
};
use crate::tool::offline_server::start_offline_server;
use crate::utils::UrlFromMultiAddr;
use ahash::HashMap;
use anyhow::{bail, ensure, Context as _};
use bls_signatures::Serialize as _;
use cid::Cid;
use clap::{Subcommand, ValueEnum};
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use fil_actors_shared::v10::runtime::DomainSeparationTag;
use futures::{stream::FuturesUnordered, StreamExt};
use fvm_ipld_blockstore::Blockstore;
use ipld_core::ipld::Ipld;
use itertools::Itertools as _;
use jsonrpsee::types::ErrorCode;
use libp2p::PeerId;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use similar::{ChangeTag, TextDiff};
use std::{
    borrow::Cow,
    io,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    time::Duration,
};
use tabled::{builder::Builder, settings::Style};
use test_snapshot::RpcTestSnapshot;
use tokio::sync::Semaphore;
use tracing::debug;

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
        /// Genesis file path, only applicable for devnet
        #[arg(long)]
        genesis: Option<PathBuf>,
        /// If provided, indicates the file to which to save the admin token.
        #[arg(long)]
        save_token: Option<PathBuf>,
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

        /// Specify a directory to which the RPC tests are dumped
        #[arg(long)]
        dump_dir: Option<PathBuf>,

        /// Additional overrides to modify success criteria for tests
        #[arg(long, value_enum, num_args = 0.., use_value_delimiter = true, value_delimiter = ',', default_values_t = [TestCriteriaOverride::TimeoutAndTimeout])]
        test_criteria_overrides: Vec<TestCriteriaOverride>,
    },
    GenerateTestSnapshot {
        /// Path to test dumps that are generated by `forest-tool api dump-tests` command
        #[arg(num_args = 1.., required = true)]
        test_dump_files: Vec<PathBuf>,
        /// Path to the database folder that powers a Forest node
        #[arg(long, required = true)]
        db: PathBuf,
        /// Filecoin network chain
        #[arg(long, required = true)]
        chain: NetworkChain,
        #[arg(long, required = true)]
        /// Folder into which test snapshots are dumped
        out_dir: PathBuf,
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
    Test {
        /// Path to test snapshots that are generated by `forest-tool api generate-test-snapshot` command
        #[arg(num_args = 1.., required = true)]
        files: Vec<PathBuf>,
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
                genesis,
                save_token,
            } => {
                if chain.is_devnet() {
                    ensure!(
                        !auto_download_snapshot,
                        "auto_download_snapshot is not supported for devnet"
                    );
                    ensure!(genesis.is_some(), "genesis must be provided for devnet");
                }

                start_offline_server(
                    snapshot_files,
                    chain,
                    port,
                    auto_download_snapshot,
                    height,
                    genesis,
                    save_token,
                )
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
                dump_dir,
                test_criteria_overrides,
            } => {
                let forest = Arc::new(rpc::Client::from_url(forest));
                let lotus = Arc::new(rpc::Client::from_url(lotus));

                for tests in [
                    create_tests(create_tests_args.clone())?,
                    create_tests_pass_2(create_tests_args)?,
                ] {
                    run_tests(
                        tests,
                        forest.clone(),
                        lotus.clone(),
                        max_concurrent_requests,
                        filter_file.clone(),
                        filter.clone(),
                        run_ignored,
                        fail_fast,
                        dump_dir.clone(),
                        &test_criteria_overrides,
                    )
                    .await?;
                }
            }
            Self::GenerateTestSnapshot {
                test_dump_files,
                db,
                chain,
                out_dir,
            } => {
                std::env::set_var("FOREST_TIPSET_CACHE_DISABLED", "1");
                if !out_dir.is_dir() {
                    std::fs::create_dir_all(&out_dir)?;
                }
                let tracking_db = generate_test_snapshot::load_db(&db)?;
                for test_dump_file in test_dump_files {
                    let out_path = out_dir
                        .join(test_dump_file.file_name().context("Infallible")?)
                        .with_extension("rpcsnap.json");
                    let test_dump = serde_json::from_reader(std::fs::File::open(&test_dump_file)?)?;
                    print!("Generating RPC snapshot at {} ...", out_path.display());
                    match generate_test_snapshot::run_test_with_dump(
                        &test_dump,
                        tracking_db.clone(),
                        &chain,
                    )
                    .await
                    {
                        Ok(_) => {
                            let snapshot = {
                                tracking_db.ensure_chain_head_is_tracked()?;
                                let mut db = vec![];
                                tracking_db.export_forest_car(&mut db).await?;
                                RpcTestSnapshot {
                                    chain: chain.clone(),
                                    name: test_dump.request.method_name.to_string(),
                                    params: test_dump.request.params,
                                    response: test_dump.forest_response,
                                    db,
                                }
                            };

                            std::fs::write(&out_path, serde_json::to_string_pretty(&snapshot)?)?;
                            println!(" Succeeded");
                        }
                        Err(e) => {
                            println!(" Failed: {e}");
                        }
                    };
                }
            }
            Self::Test { files } => {
                for path in files {
                    print!("Running RPC test with snapshot {} ...", path.display());
                    match test_snapshot::run_test_from_snapshot(&path).await {
                        Ok(_) => {
                            println!("  Succeeded");
                        }
                        Err(e) => {
                            println!(" Failed: {e}");
                        }
                    };
                }
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

#[derive(clap::Args, Debug, Clone)]
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

#[derive(Debug, Copy, Clone, PartialEq, ValueEnum)]
pub enum TestCriteriaOverride {
    /// Test pass when first endpoint returns a valid result and the second one timeout
    ValidAndTimeout,
    /// Test pass when both endpoints timeout
    TimeoutAndTimeout,
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

#[derive(ValueEnum, Debug, Clone, Copy)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestDump {
    request: rpc::Request,
    forest_response: Result<serde_json::Value, String>,
    lotus_response: Result<serde_json::Value, String>,
}

impl std::fmt::Display for TestDump {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Request dump: {:?}", self.request)?;
        writeln!(f, "Request params JSON: {}", self.request.params)?;
        let (forest_response, lotus_response) = (
            self.forest_response
                .as_ref()
                .ok()
                .and_then(|v| serde_json::to_string_pretty(v).ok()),
            self.lotus_response
                .as_ref()
                .ok()
                .and_then(|v| serde_json::to_string_pretty(v).ok()),
        );
        if let (Some(forest_response), Some(lotus_response)) = (&forest_response, &lotus_response) {
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
            if let Some(forest_response) = &forest_response {
                writeln!(f, "Forest response: {}", forest_response)?;
            }
            if let Some(lotus_response) = &lotus_response {
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
    /// If Forest reason is a subset of Lotus reason, the test passes.
    /// We don't always bubble up errors and format the error chain like Lotus.
    PassWithQuasiIdenticalError,
}

enum SortPolicy {
    /// Recursively sorts both arrays and maps in a JSON value.
    All,
}

struct RpcTest {
    request: rpc::Request,
    check_syntax: Arc<dyn Fn(serde_json::Value) -> bool + Send + Sync>,
    check_semantics: Arc<dyn Fn(serde_json::Value, serde_json::Value) -> bool + Send + Sync>,
    ignore: Option<&'static str>,
    policy_on_rejected: PolicyOnRejected,
    sort_policy: Option<SortPolicy>,
}

fn sort_json(value: &mut Value) {
    match value {
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                sort_json(v);
            }
            arr.sort_by_key(|a| a.to_string());
        }
        Value::Object(obj) => {
            let mut sorted_map: serde_json::Map<String, Value> = serde_json::Map::new();
            let mut keys: Vec<String> = obj.keys().cloned().collect();
            keys.sort();
            for k in keys {
                let mut v = obj.remove(&k).unwrap();
                sort_json(&mut v);
                sorted_map.insert(k, v);
            }
            *obj = sorted_map;
        }
        _ => (),
    }
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
            sort_policy: None,
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
            sort_policy: None,
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

    fn sort_policy(mut self, policy: SortPolicy) -> Self {
        self.sort_policy = Some(policy);
        self
    }

    async fn run(&self, forest: &rpc::Client, lotus: &rpc::Client) -> TestResult {
        let forest_resp = forest.call(self.request.clone()).await;
        let forest_response = forest_resp.as_ref().map_err(|e| e.to_string()).cloned();
        let lotus_resp = lotus.call(self.request.clone()).await;
        let lotus_response = lotus_resp.as_ref().map_err(|e| e.to_string()).cloned();

        let (forest_status, lotus_status) = match (forest_resp, lotus_resp) {
            (Ok(forest), Ok(lotus))
                if (self.check_syntax)(forest.clone()) && (self.check_syntax)(lotus.clone()) =>
            {
                let (forest, lotus) = if self.sort_policy.is_some() {
                    let mut sorted_forest = forest.clone();
                    sort_json(&mut sorted_forest);
                    let mut sorted_lotus = lotus.clone();
                    sort_json(&mut sorted_lotus);
                    (sorted_forest, sorted_lotus)
                } else {
                    (forest, lotus)
                };
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

        TestResult {
            forest_status,
            lotus_status,
            test_dump: Some(TestDump {
                request: self.request.clone(),
                forest_response,
                lotus_response,
            }),
        }
    }
}

fn common_tests() -> Vec<RpcTest> {
    vec![
        // We don't check the `version` field as it differs between Lotus and Forest.
        RpcTest::validate(Version::request(()).unwrap(), |forest, lotus| {
            forest.api_version == lotus.api_version && forest.block_delay == lotus.block_delay
        }),
        RpcTest::basic(StartTime::request(()).unwrap()),
        RpcTest::basic(Session::request(()).unwrap()),
    ]
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

fn auth_tests() -> anyhow::Result<Vec<RpcTest>> {
    // Note: The second optional parameter of `AuthNew` is not supported in Lotus
    Ok(vec![
        RpcTest::basic(AuthNew::request((
            AuthNewParams::process_perms(Permission::Admin.to_string())?,
            None,
        ))?),
        RpcTest::basic(AuthNew::request((
            AuthNewParams::process_perms(Permission::Sign.to_string())?,
            None,
        ))?),
        RpcTest::basic(AuthNew::request((
            AuthNewParams::process_perms(Permission::Write.to_string())?,
            None,
        ))?),
        RpcTest::basic(AuthNew::request((
            AuthNewParams::process_perms(Permission::Read.to_string())?,
            None,
        ))?),
    ])
}

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
    // More net commands should be tested. Tracking issue:
    // https://github.com/ChainSafe/forest/issues/3639
    vec![
        RpcTest::basic(NetAddrsListen::request(()).unwrap()),
        RpcTest::basic(NetPeers::request(()).unwrap()),
        RpcTest::identity(NetListening::request(()).unwrap()),
        // Tests with a known peer id tend to be flaky, use a random peer id to test the unhappy path only
        RpcTest::basic(NetAgentVersion::request((PeerId::random().to_string(),)).unwrap())
            .policy_on_rejected(PolicyOnRejected::PassWithIdenticalError),
        RpcTest::basic(NetFindPeer::request((PeerId::random().to_string(),)).unwrap())
            .policy_on_rejected(PolicyOnRejected::Pass)
            .ignore("It times out in lotus when peer not found"),
        RpcTest::basic(NetInfo::request(()).unwrap())
            .ignore("Not implemented in Lotus. Why do we even have this method?"),
        RpcTest::basic(NetAutoNatStatus::request(()).unwrap()),
        RpcTest::identity(NetVersion::request(()).unwrap()),
        RpcTest::identity(NetProtectAdd::request((vec![PeerId::random().to_string()],)).unwrap()),
        RpcTest::identity(
            NetProtectRemove::request((vec![PeerId::random().to_string()],)).unwrap(),
        ),
        RpcTest::basic(NetProtectList::request(()).unwrap()),
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
    // TODO(forest): https://github.com/ChainSafe/forest/issues/4718
    // Blocked by Lotus.
    //vec![RpcTest::identity(
    //    BeaconGetEntry::request((10101,)).unwrap(),
    //)]
    vec![]
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
        RpcTest::identity(StateMinerInitialPledgeForSector::request((
            1,
            SectorSize::_32GiB,
            1024,
            tipset.key().into(),
        ))?),
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
        RpcTest::identity(BeaconGetEntry::request((tipset.epoch(),))?),
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
                validate_message_lookup(StateSearchMsg::request((
                    None.into(),
                    msg_cid,
                    800,
                    true,
                ))?),
                validate_message_lookup(StateSearchMsg::request((
                    None.into(),
                    msg_cid,
                    800,
                    false,
                ))?),
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
        tests.push(RpcTest::identity(
            EthCall::request_with_alias(
                (
                    EthCallMessage {
                        to: Some(
                            EthAddress::from_str("0x0c1d86d34e469770339b53613f3a2343accd62cb")
                                .unwrap(),
                        ),
                        data: "0xf8b2cb4f000000000000000000000000CbfF24DED1CE6B53712078759233Ac8f91ea71B6".parse().unwrap(),
                        ..EthCallMessage::default()
                    },
                    BlockNumberOrHash::from_predefined(Predefined::Latest),
                ),
                use_alias,
            )
            .unwrap(),
        ));
        tests.push(RpcTest::basic(
            EthNewFilter::request_with_alias(
                (EthFilterSpec {
                    from_block: None,
                    to_block: None,
                    address: vec![EthAddress::from_str(
                        "0xff38c072f286e3b20b3954ca9f99c05fbecc64aa",
                    )
                    .unwrap()],
                    topics: None,
                    block_hash: None,
                },),
                use_alias,
            )
            .unwrap(),
        ));
        tests.push(RpcTest::basic(
            EthNewPendingTransactionFilter::request_with_alias((), use_alias).unwrap(),
        ));
        tests.push(RpcTest::basic(
            EthNewBlockFilter::request_with_alias((), use_alias).unwrap(),
        ));
        tests.push(RpcTest::identity(
            EthUninstallFilter::request_with_alias((FilterID::new().unwrap(),), use_alias).unwrap(),
        ));
        tests.push(RpcTest::identity(
            EthAddressToFilecoinAddress::request((EthAddress::from_str(
                "0xff38c072f286e3b20b3954ca9f99c05fbecc64aa",
            )
            .unwrap(),))
            .unwrap(),
        ));
    }
    tests
}

fn eth_tests_with_tipset<DB: Blockstore>(store: &Arc<DB>, shared_tipset: &Tipset) -> Vec<RpcTest> {
    let block_cid = shared_tipset.key().cid().unwrap();
    let block_hash: EthHash = block_cid.into();

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
        RpcTest::basic(
            EthGetBlockByNumber::request((
                BlockNumberOrHash::from_predefined(Predefined::Safe),
                true,
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetBlockByNumber::request((
                BlockNumberOrHash::from_predefined(Predefined::Finalized),
                true,
            ))
            .unwrap(),
        ),
        RpcTest::identity(EthGetBlockReceipts::request((block_hash.clone(),)).unwrap()),
        RpcTest::identity(
            EthGetBlockTransactionCountByHash::request((block_hash.clone(),)).unwrap(),
        ),
        RpcTest::identity(EthGetBlockReceiptsLimited::request((block_hash.clone(), 800)).unwrap()),
        RpcTest::identity(
            EthGetBlockTransactionCountByNumber::request((EthInt64(shared_tipset.epoch()),))
                .unwrap(),
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
                BlockNumberOrHash::BlockNumber(EthInt64(shared_tipset.epoch())),
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
            EthGetTransactionByBlockNumberAndIndex::request((
                BlockNumberOrPredefined::BlockNumber(shared_tipset.epoch().into()),
                0.into(),
            ))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithIdenticalError),
        RpcTest::identity(
            EthGetTransactionByBlockHashAndIndex::request((block_hash.clone(), 0.into())).unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithIdenticalError),
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
        RpcTest::identity(
            EthGetLogs::request((EthFilterSpec {
                from_block: Some(format!("0x{:x}", shared_tipset.epoch())),
                to_block: Some(format!("0x{:x}", shared_tipset.epoch())),
                address: vec![],
                topics: None,
                block_hash: None,
            },))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetLogs::request((EthFilterSpec {
                from_block: Some(format!("0x{:x}", shared_tipset.epoch() - 100)),
                to_block: Some(format!("0x{:x}", shared_tipset.epoch())),
                address: vec![],
                topics: None,
                block_hash: None,
            },))
            .unwrap(),
        )
        .sort_policy(SortPolicy::All),
        RpcTest::identity(EthGetTransactionHashByCid::request((block_cid,)).unwrap()),
        RpcTest::identity(
            EthTraceBlock::request((BlockNumberOrHash::from_block_number(shared_tipset.epoch()),))
                .unwrap(),
        ),
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
                EthGetMessageCidByTransactionHash::request((tx.hash.clone(),))?,
            ));
            tests.push(RpcTest::identity(EthGetTransactionByHash::request((tx
                .hash
                .clone(),))?));
            tests.push(RpcTest::identity(EthGetTransactionByHashLimited::request(
                (tx.hash.clone(), shared_tipset.epoch()),
            )?));
            if smsg.message.from.protocol() == Protocol::Delegated
                && smsg.message.to.protocol() == Protocol::Delegated
            {
                tests.push(
                    RpcTest::identity(EthGetTransactionReceipt::request((tx.hash.clone(),))?)
                        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
                );
                tests.push(
                    RpcTest::identity(EthGetTransactionReceiptLimited::request((tx.hash, 800))?)
                        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
                );
            }
        }
    }
    tests.push(RpcTest::identity(
        EthGetMessageCidByTransactionHash::request((EthHash::from_str(
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

fn f3_tests() -> anyhow::Result<Vec<RpcTest>> {
    Ok(vec![
        // using basic because 2 nodes are not garanteed to be at the same head
        RpcTest::basic(F3GetECPowerTable::request((None.into(),))?),
        RpcTest::basic(F3GetLatestCertificate::request(())?),
        RpcTest::basic(F3ListParticipants::request(())?),
        RpcTest::basic(F3GetProgress::request(())?),
        RpcTest::basic(F3GetOrRenewParticipationTicket::request((
            Address::new_id(1000),
            vec![],
            3,
        ))?),
        RpcTest::identity(F3IsRunning::request(())?),
        RpcTest::identity(F3GetCertificate::request((0,))?),
        RpcTest::identity(F3GetCertificate::request((1000,))?),
        RpcTest::identity(F3GetManifest::request(())?),
    ])
}

fn f3_tests_with_tipset(tipset: &Tipset) -> anyhow::Result<Vec<RpcTest>> {
    Ok(vec![
        RpcTest::identity(F3GetECPowerTable::request((tipset.key().into(),))?),
        RpcTest::identity(F3GetF3PowerTable::request((tipset.key().into(),))?),
    ])
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
    // use heaviest - SAFE_EPOCH_DELAY instead
    let shared_tipset = store
        .heaviest_tipset()?
        .chain(&store)
        .take(SAFE_EPOCH_DELAY as usize)
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
        tests.extend(f3_tests_with_tipset(&tipset)?);
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
    tests.extend(auth_tests()?);
    tests.extend(common_tests());
    tests.extend(chain_tests());
    tests.extend(mpool_tests());
    tests.extend(net_tests());
    tests.extend(node_tests());
    tests.extend(wallet_tests(worker_address));
    tests.extend(eth_tests());
    tests.extend(state_tests());
    tests.extend(f3_tests()?);
    if !snapshot_files.is_empty() {
        let store = Arc::new(ManyCar::try_from(snapshot_files)?);
        tests.extend(snapshot_tests(
            store,
            n_tipsets,
            miner_address,
            eth_chain_id,
        )?);
    }
    tests.sort_by_key(|test| test.request.method_name.clone());
    Ok(tests)
}

fn create_tests_pass_2(
    CreateTestsArgs { snapshot_files, .. }: CreateTestsArgs,
) -> anyhow::Result<Vec<RpcTest>> {
    let mut tests = vec![];

    if !snapshot_files.is_empty() {
        let store = Arc::new(ManyCar::try_from(snapshot_files)?);
        tests.push(RpcTest::identity(ChainSetHead::request((store
            .heaviest_tipset()?
            .key()
            .clone(),))?));
    }

    Ok(tests)
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
    dump_dir: Option<PathBuf>,
    test_criteria_overrides: &[TestCriteriaOverride],
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
         }| {
            (
                method_name.clone(),
                params.clone(),
                *api_paths,
                ignore.is_some(),
            )
        },
    ) {
        // By default, do not run ignored tests.
        if matches!(run_ignored, RunIgnored::Default) && test.ignore.is_some() {
            continue;
        }
        // If in `IgnoreOnly` mode, only run ignored tests.
        if matches!(run_ignored, RunIgnored::IgnoredOnly) && test.ignore.is_none() {
            continue;
        }

        if !filter_list.authorize(&test.request.method_name) {
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
            (TestSummary::Valid, TestSummary::Valid) => true,
            (TestSummary::Valid, TestSummary::Timeout) => {
                test_criteria_overrides.contains(&TestCriteriaOverride::ValidAndTimeout)
            }
            (TestSummary::Timeout, TestSummary::Timeout) => {
                test_criteria_overrides.contains(&TestCriteriaOverride::TimeoutAndTimeout)
            }
            (TestSummary::Rejected(ref reason_forest), TestSummary::Rejected(ref reason_lotus)) => {
                match test.policy_on_rejected {
                    PolicyOnRejected::Pass => true,
                    PolicyOnRejected::PassWithIdenticalError if reason_forest == reason_lotus => {
                        true
                    }
                    PolicyOnRejected::PassWithQuasiIdenticalError
                        if reason_lotus.starts_with(reason_forest) =>
                    {
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        };

        if let (Some(dump_dir), Some(test_dump)) = (&dump_dir, &test_result.test_dump) {
            let dir = dump_dir.join(if success { "valid" } else { "invalid" });
            if !dir.is_dir() {
                std::fs::create_dir_all(&dir)?;
            }
            let filename = format!(
                "{}_{}.json",
                test_dump
                    .request
                    .method_name
                    .as_ref()
                    .replace(".", "_")
                    .to_lowercase(),
                chrono::Utc::now().timestamp_micros()
            );
            std::fs::write(dir.join(filename), serde_json::to_string_pretty(test_dump)?)?;
        }

        let result_entry = (method_name, forest_status, lotus_status);
        if success {
            success_results
                .entry(result_entry)
                .and_modify(|v| *v += 1)
                .or_insert(1u32);
        } else {
            if let Some(test_dump) = test_result.test_dump {
                fail_details.push(test_dump);
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
    success_results: &HashMap<(Cow<'static, str>, TestSummary, TestSummary), u32>,
    failed_results: &HashMap<(Cow<'static, str>, TestSummary, TestSummary), u32>,
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

#[allow(clippy::type_complexity)]
fn format_as_markdown(results: &[((Cow<'static, str>, TestSummary, TestSummary), u32)]) -> String {
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
    fn authorize(&self, entry: impl AsRef<str>) -> bool {
        let entry = entry.as_ref();
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
