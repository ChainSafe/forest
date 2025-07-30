// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod api_compare_tests;
mod api_run_tests;
mod generate_test_snapshot;
mod report;
mod test_snapshot;

use crate::cli_shared::{chain_path, read_config};
use crate::db::car::ManyCar;
use crate::db::db_engine::db_root;
use crate::eth::EthChainId as EthChainIdType;
use crate::lotus_json::HasLotusJson;
use crate::networks::NetworkChain;
use crate::rpc;
use crate::rpc::eth::types::*;
use crate::rpc::prelude::*;
use crate::shim::address::Address;
use crate::tool::offline_server::start_offline_server;
use crate::tool::subcommands::api_cmd::api_run_tests::TestTransaction;
use crate::tool::subcommands::api_cmd::test_snapshot::{Index, Payload};
use crate::utils::UrlFromMultiAddr;
use anyhow::{Context as _, bail, ensure};
use cid::Cid;
use clap::{Subcommand, ValueEnum};
use fvm_ipld_blockstore::Blockstore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::str::FromStr;
use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};
use test_snapshot::RpcTestSnapshot;

#[derive(Debug, Copy, Clone, PartialEq, ValueEnum)]
pub enum NodeType {
    Forest,
    Lotus,
}

/// Report mode for the API compare tests.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ReportMode {
    /// Show everything
    Full,
    /// Show summary and failures only
    FailureOnly,
    /// Show summary only
    Summary,
}

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

        /// Specify a directory to dump the test report
        #[arg(long)]
        report_dir: Option<PathBuf>,

        /// Report detail level: full (default), failure-only, or summary
        #[arg(long, value_enum, default_value = "full")]
        report_mode: ReportMode,
    },
    GenerateTestSnapshot {
        /// Path to test dumps that are generated by `forest-tool api dump-tests` command
        #[arg(num_args = 1.., required = true)]
        test_dump_files: Vec<PathBuf>,
        /// Path to the database folder that powers a Forest node
        #[arg(long)]
        db: Option<PathBuf>,
        /// Filecoin network chain
        #[arg(long, required = true)]
        chain: NetworkChain,
        #[arg(long, required = true)]
        /// Folder into which test snapshots are dumped
        out_dir: PathBuf,
        /// Allow generating snapshot even if Lotus generated a different response. This is useful
        /// when the response is not deterministic or a failing test is expected.
        /// If generating a failing test, use `Lotus` as the argument to ensure the test passes
        /// only when the response from Forest is fixed and matches the response from Lotus.
        #[arg(long)]
        use_response_from: Option<NodeType>,
    },
    DumpTests {
        #[command(flatten)]
        create_tests_args: CreateTestsArgs,
        /// Which API path to dump.
        #[arg(long)]
        path: rpc::ApiPaths,
        #[arg(long)]
        include_ignored: bool,
    },
    Test {
        /// Path to test snapshots that are generated by `forest-tool api generate-test-snapshot` command
        #[arg(num_args = 1.., required = true)]
        files: Vec<PathBuf>,
    },
    Run {
        /// Client address
        addr: UrlFromMultiAddr,
        /// Test Transaction to address
        #[arg(long)]
        to: String,
        /// Test Transaction from address
        #[arg(long)]
        from: String,
        /// Test Transaction hex payload
        #[arg(long)]
        payload: String,
        /// Log topic to search for
        #[arg(long)]
        topic: String,
        /// Filter which tests to run according to method name. Case sensitive.
        #[arg(long, default_value = "")]
        filter: String,
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
                report_dir,
                report_mode,
            } => {
                let forest = Arc::new(rpc::Client::from_url(forest));
                let lotus = Arc::new(rpc::Client::from_url(lotus));
                let tests = api_compare_tests::create_tests(create_tests_args.clone()).await?;

                api_compare_tests::run_tests(
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
                    report_dir.clone(),
                    report_mode,
                )
                .await?;
            }
            Self::GenerateTestSnapshot {
                test_dump_files,
                db,
                chain,
                out_dir,
                use_response_from,
            } => {
                unsafe { std::env::set_var("FOREST_TIPSET_CACHE_DISABLED", "1") };
                if !out_dir.is_dir() {
                    std::fs::create_dir_all(&out_dir)?;
                }
                let db = if let Some(db) = db {
                    db
                } else {
                    let (_, config) = read_config(None, Some(chain.clone()))?;
                    db_root(&chain_path(&config))?
                };
                let tracking_db = generate_test_snapshot::load_db(&db)?;
                for test_dump_file in test_dump_files {
                    let out_path = out_dir
                        .join(test_dump_file.file_name().context("Infallible")?)
                        .with_extension("rpcsnap.json");
                    let test_dump = serde_json::from_reader(std::fs::File::open(&test_dump_file)?)?;
                    print!("Generating RPC snapshot at {} ...", out_path.display());
                    let allow_response_mismatch = use_response_from.is_some();
                    match generate_test_snapshot::run_test_with_dump(
                        &test_dump,
                        tracking_db.clone(),
                        &chain,
                        allow_response_mismatch,
                    )
                    .await
                    {
                        Ok(_) => {
                            let snapshot = {
                                tracking_db.ensure_chain_head_is_tracked()?;
                                let mut db = vec![];
                                tracking_db.export_forest_car(&mut db).await?;
                                let index =
                                    generate_test_snapshot::build_index(tracking_db.clone());
                                RpcTestSnapshot {
                                    chain: chain.clone(),
                                    name: test_dump.request.method_name.to_string(),
                                    params: test_dump.request.params,
                                    response: match use_response_from {
                                        Some(NodeType::Forest) | None => test_dump.forest_response,
                                        Some(NodeType::Lotus) => test_dump.lotus_response,
                                    },
                                    index,
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
                    let start = Instant::now();
                    match test_snapshot::run_test_from_snapshot(&path).await {
                        Ok(_) => {
                            println!(
                                "  succeeded, took {}.",
                                humantime::format_duration(start.elapsed())
                            );
                        }
                        Err(e) => {
                            println!(" Failed: {e}");
                        }
                    };
                }
            }
            Self::Run {
                addr: UrlFromMultiAddr(url),
                to,
                from,
                payload,
                topic,
                filter,
            } => {
                let client = Arc::new(rpc::Client::from_url(url));

                let to = Address::from_str(&to)?;
                let from = Address::from_str(&from)?;
                let payload = hex::decode(payload)?;
                let topic = EthHash::from_str(&topic)?;
                let tx = TestTransaction {
                    to,
                    from,
                    payload,
                    topic,
                };

                let tests = api_run_tests::create_tests(tx).await;
                api_run_tests::run_tests(tests, client.clone(), filter).await?;
            }
            Self::DumpTests {
                create_tests_args,
                path,
                include_ignored,
            } => {
                for api_compare_tests::RpcTest {
                    request:
                        rpc::Request {
                            method_name,
                            params,
                            api_paths,
                            ..
                        },
                    ignore,
                    ..
                } in api_compare_tests::create_tests(create_tests_args).await?
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
    #[arg(long, default_value_t = crate::networks::calibnet::ETH_CHAIN_ID)]
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
