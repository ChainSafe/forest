// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::Tipset;
use crate::chain::ChainStore;
use crate::chain_sync::{SyncConfig, SyncStage};
use crate::cli_shared::snapshot::TrustedVendor;
use crate::daemon::db_util::download_to;
use crate::db::{car::ManyCar, MemoryDB};
use crate::genesis::{get_network_name_from_genesis, read_genesis_header};
use crate::key_management::{KeyStore, KeyStoreConfig};
use crate::lotus_json::{HasLotusJson, LotusJson};
use crate::message::Message as _;
use crate::message_pool::{MessagePool, MpoolRpcProvider};
use crate::networks::{parse_bootstrap_peers, ChainConfig, NetworkChain};
use crate::rpc::beacon::BeaconGetEntry;
use crate::rpc::eth::Address as EthAddress;
use crate::rpc::eth::*;
use crate::rpc::gas::GasEstimateGasLimit;
use crate::rpc::types::{ApiTipsetKey, MessageFilter, MessageLookup};
use crate::rpc::{prelude::*, start_rpc, RPCState, ServerError};
use crate::rpc_client::{ApiInfo, RpcRequest, DEFAULT_PORT};
use crate::shim::address::{CurrentNetwork, Network};
use crate::shim::{
    address::{Address, Protocol},
    crypto::Signature,
    econ::TokenAmount,
    message::{Message, METHOD_SEND},
    state_tree::StateTree,
};
use crate::state_manager::StateManager;
use ahash::HashMap;
use anyhow::Context as _;
use clap::{Subcommand, ValueEnum};
use fil_actor_interface::market;
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use fil_actors_shared::v10::runtime::DomainSeparationTag;
use futures::{stream::FuturesUnordered, StreamExt};
use fvm_ipld_blockstore::Blockstore;
use fvm_shared2::piece::PaddedPieceSize;
use itertools::Itertools as _;
use jsonrpsee::types::ErrorCode;
use serde::de::DeserializeOwned;
use similar::{ChangeTag, TextDiff};
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
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        // Allow downloading snapshot automatically
        #[arg(long)]
        auto_download_snapshot: bool,
        /// Validate snapshot at given EPOCH, use a negative value -N to validate
        /// the last N EPOCH(s) starting at HEAD.
        #[arg(long, default_value_t = -50)]
        height: i64,
    },
    /// Compare
    Compare {
        /// Forest address
        #[clap(long, default_value_t = ApiInfo::from_str("/ip4/127.0.0.1/tcp/2345/http").expect("infallible"))]
        forest: ApiInfo,
        /// Lotus address
        #[clap(long, default_value_t = ApiInfo::from_str("/ip4/127.0.0.1/tcp/1234/http").expect("infallible"))]
        lotus: ApiInfo,
        /// Snapshot input paths. Supports `.car`, `.car.zst`, and `.forest.car.zst`.
        #[arg()]
        snapshot_files: Vec<PathBuf>,
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
        #[arg(short, long, default_value = "20")]
        /// The number of tipsets to use to generate test cases.
        n_tipsets: usize,
        #[arg(long, value_enum, default_value_t = RunIgnored::Default)]
        /// Behavior for tests marked as `ignored`.
        run_ignored: RunIgnored,
        /// Maximum number of concurrent requests
        #[arg(long, default_value = "8")]
        max_concurrent_requests: usize,
    },
}

/// For more information about each flag, refer to the Forest documentation at:
/// <https://docs.forest.chainsafe.io/rustdoc/forest_filecoin/tool/subcommands/api_cmd/enum.ApiCommands.html>
struct ApiTestFlags {
    filter: String,
    filter_file: Option<PathBuf>,
    fail_fast: bool,
    n_tipsets: usize,
    run_ignored: RunIgnored,
    max_concurrent_requests: usize,
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
                forest,
                lotus,
                snapshot_files,
                filter,
                filter_file,
                fail_fast,
                n_tipsets,
                run_ignored,
                max_concurrent_requests,
            } => {
                let config = ApiTestFlags {
                    filter,
                    filter_file,
                    fail_fast,
                    n_tipsets,
                    run_ignored,
                    max_concurrent_requests,
                };

                compare_apis(forest, lotus, snapshot_files, config).await?
            }
        }
        Ok(())
    }
}

#[derive(ValueEnum, Debug, Clone)]
#[clap(rename_all = "kebab_case")]
pub enum RunIgnored {
    Default,
    IgnoredOnly,
    All,
}

#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Hash)]
enum EndpointStatus {
    // RPC method is missing
    MissingMethod,
    // Request isn't valid according to jsonrpc spec
    InvalidRequest,
    // Catch-all for errors on the node
    InternalServerError,
    // Unexpected JSON schema
    InvalidJSON,
    // Got response with the right JSON schema but it failed sanity checking
    InvalidResponse,
    Timeout,
    Valid,
}

impl EndpointStatus {
    fn from_json_error(err: ServerError) -> Self {
        match err.known_code() {
            ErrorCode::ParseError => Self::InvalidResponse,
            ErrorCode::OversizedRequest => Self::InvalidRequest,
            ErrorCode::InvalidRequest => Self::InvalidRequest,
            ErrorCode::MethodNotFound => Self::MissingMethod,
            it if it.code() == 0 && it.message().contains("timed out") => Self::Timeout,
            _ => {
                tracing::debug!(?err);
                Self::InternalServerError
            }
        }
    }
}

/// Data about a failed test. Used for debugging.
struct TestDump {
    request: RpcRequest,
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
    forest_status: EndpointStatus,
    /// Lotus result after calling the RPC method.
    lotus_status: EndpointStatus,
    /// Optional data dump if either status was invalid.
    test_dump: Option<TestDump>,
}

struct RpcTest {
    request: RpcRequest,
    check_syntax: Arc<dyn Fn(serde_json::Value) -> bool + Send + Sync>,
    check_semantics: Arc<dyn Fn(serde_json::Value, serde_json::Value) -> bool + Send + Sync>,
    ignore: Option<&'static str>,
}

/// Duplication between `<method>` and `<method>_raw` is a temporary measure, and
/// should be removed when <https://github.com/ChainSafe/forest/issues/4032> is
/// completed.
impl RpcTest {
    /// Check that an endpoint exists and that both the Lotus and Forest JSON
    /// response follows the same schema.
    fn basic<T>(request: RpcRequest<T>) -> Self
    where
        T: HasLotusJson,
    {
        Self::basic_raw(request.map_ty::<T::LotusJson>())
    }
    /// See [Self::basic], and note on this `impl` block.
    fn basic_raw<T: DeserializeOwned>(request: RpcRequest<T>) -> Self {
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
        }
    }
    /// Check that an endpoint exists, has the same JSON schema, and do custom
    /// validation over both responses.
    fn validate<T: HasLotusJson>(
        request: RpcRequest<T>,
        validate: impl Fn(T, T) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self::validate_raw(request.map_ty::<T::LotusJson>(), move |l, r| {
            validate(T::from_lotus_json(l), T::from_lotus_json(r))
        })
    }
    /// See [Self::validate], and note on this `impl` block.
    fn validate_raw<T: DeserializeOwned>(
        request: RpcRequest<T>,
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
        }
    }
    /// Check that an endpoint exists and that Forest returns exactly the same
    /// JSON as Lotus.
    fn identity<T: PartialEq + HasLotusJson>(request: RpcRequest<T>) -> RpcTest {
        Self::validate(request, |forest, lotus| forest == lotus)
    }
    /// See [Self::identity], and note on this `impl` block.
    fn identity_raw<T: PartialEq + DeserializeOwned>(request: RpcRequest<T>) -> Self {
        Self::validate_raw(request, |l, r| l == r)
    }

    fn with_timeout(mut self, timeout: Duration) -> Self {
        self.request.set_timeout(timeout);
        self
    }

    fn ignore(mut self, msg: &'static str) -> Self {
        self.ignore = Some(msg);
        self
    }

    async fn run(&self, forest_api: &ApiInfo, lotus_api: &ApiInfo) -> TestResult {
        let forest_resp = forest_api.call(self.request.clone()).await;
        let lotus_resp = lotus_api.call(self.request.clone()).await;

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
                    EndpointStatus::Valid
                } else {
                    EndpointStatus::InvalidResponse
                };
                (forest_status, EndpointStatus::Valid)
            }
            (forest_resp, lotus_resp) => {
                let forest_status =
                    forest_resp.map_or_else(EndpointStatus::from_json_error, |value| {
                        if (self.check_syntax)(value) {
                            EndpointStatus::Valid
                        } else {
                            EndpointStatus::InvalidJSON
                        }
                    });
                let lotus_status =
                    lotus_resp.map_or_else(EndpointStatus::from_json_error, |value| {
                        if (self.check_syntax)(value) {
                            EndpointStatus::Valid
                        } else {
                            EndpointStatus::InvalidJSON
                        }
                    });

                (forest_status, lotus_status)
            }
        };

        if forest_status == EndpointStatus::Valid && lotus_status == EndpointStatus::Valid {
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
        RpcTest::basic_raw(Version::request(()).unwrap()),
        RpcTest::basic_raw(StartTime::request(()).unwrap()),
        RpcTest::basic_raw(Session::request(()).unwrap()),
    ]
}

fn auth_tests() -> Vec<RpcTest> {
    // Auth commands should be tested as well. Tracking issue:
    // https://github.com/ChainSafe/forest/issues/3639
    vec![]
}

fn beacon_tests() -> Vec<RpcTest> {
    vec![RpcTest::identity_raw(
        BeaconGetEntry::request((10101,)).unwrap(),
    )]
}

fn chain_tests() -> Vec<RpcTest> {
    vec![
        RpcTest::basic_raw(ChainHead::request(()).unwrap()),
        RpcTest::identity_raw(ChainGetGenesis::request(()).unwrap()),
    ]
}

fn chain_tests_with_tipset(shared_tipset: &Tipset) -> Vec<RpcTest> {
    let shared_block_cid = (*shared_tipset.min_ticket_block().cid()).into();

    vec![
        RpcTest::identity_raw(ChainReadObj::request((shared_block_cid,)).unwrap()),
        RpcTest::identity_raw(ChainHasObj::request((shared_block_cid,)).unwrap()),
        RpcTest::identity_raw(ChainGetBlock::request((shared_block_cid,)).unwrap()),
        RpcTest::identity_raw(
            ChainGetTipSetAfterHeight::request((shared_tipset.epoch(), Default::default()))
                .unwrap(),
        ),
        RpcTest::identity_raw(
            ChainGetTipSetAfterHeight::request((shared_tipset.epoch(), Default::default()))
                .unwrap(),
        ),
        RpcTest::identity_raw(
            ChainGetTipSet::request((LotusJson(shared_tipset.key().clone().into()),)).unwrap(),
        ),
        RpcTest::identity_raw(
            ChainGetPath::request((
                shared_tipset.key().clone().into(),
                shared_tipset.parents().clone().into(),
            ))
            .unwrap(),
        ),
    ]
}

fn mpool_tests() -> Vec<RpcTest> {
    vec![RpcTest::basic_raw(
        MpoolPending::request((LotusJson(ApiTipsetKey(None)),)).unwrap(),
    )]
}

fn net_tests() -> Vec<RpcTest> {
    let bootstrap_peers = parse_bootstrap_peers(include_str!("../../../build/bootstrap/calibnet"));
    let peer_id = bootstrap_peers
        .last()
        .expect("No bootstrap peers found - bootstrap file is empty or corrupted")
        .to_string()
        .rsplit_once('/')
        .expect("No peer id found - address is not in the expected format")
        .1
        .to_string();

    // More net commands should be tested. Tracking issue:
    // https://github.com/ChainSafe/forest/issues/3639
    vec![
        RpcTest::basic_raw(NetAddrsListen::request(()).unwrap()),
        RpcTest::basic_raw(NetPeers::request(()).unwrap()),
        RpcTest::identity_raw(NetListening::request(()).unwrap()),
        RpcTest::basic_raw(NetAgentVersion::request((peer_id,)).unwrap()),
        RpcTest::basic_raw(NetInfo::request(()).unwrap())
            .ignore("Not implemented in Lotus. Why do we even have this method?"),
        RpcTest::basic_raw(NetAutoNatStatus::request(()).unwrap()),
        RpcTest::identity_raw(NetVersion::request(()).unwrap()),
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
        RpcTest::identity_raw(StateGetBeaconEntry::request((0.into(),)).unwrap()),
        RpcTest::identity_raw(StateGetBeaconEntry::request((1.into(),)).unwrap()),
    ]
}

fn state_tests_with_tipset(shared_tipset: &Tipset) -> Vec<RpcTest> {
    let shared_block = shared_tipset.min_ticket_block();
    let mut sectors = BitField::new();
    sectors.set(101);
    vec![
        RpcTest::identity(ApiInfo::state_network_name_req()),
        RpcTest::identity(ApiInfo::state_get_actor_req(
            Address::SYSTEM_ACTOR,
            shared_tipset.key().clone(),
        )),
        RpcTest::identity(ApiInfo::state_get_randomness_from_tickets_req(
            shared_tipset.key().into(),
            DomainSeparationTag::ElectionProofProduction,
            shared_tipset.epoch(),
            "dead beef".as_bytes().to_vec(),
        )),
        RpcTest::identity(ApiInfo::state_get_randomness_from_beacon_req(
            shared_tipset.key().into(),
            DomainSeparationTag::ElectionProofProduction,
            shared_tipset.epoch(),
            "dead beef".as_bytes().to_vec(),
        )),
        RpcTest::identity(ApiInfo::state_read_state_req(
            Address::SYSTEM_ACTOR,
            shared_tipset.key().into(),
        )),
        RpcTest::identity(ApiInfo::state_read_state_req(
            Address::SYSTEM_ACTOR,
            Default::default(),
        )),
        RpcTest::identity(ApiInfo::state_miner_active_sectors_req(
            shared_block.miner_address,
            shared_tipset.key().into(),
        )),
        RpcTest::identity(ApiInfo::state_lookup_id_req(
            shared_block.miner_address,
            shared_tipset.key().into(),
        )),
        // This should return `Address::new_id(0xdeadbeef)`
        RpcTest::identity(ApiInfo::state_lookup_id_req(
            Address::new_id(0xdeadbeef),
            shared_tipset.key().into(),
        )),
        RpcTest::identity(ApiInfo::state_network_version_req(
            shared_tipset.key().into(),
        )),
        RpcTest::identity(ApiInfo::state_list_miners_req(shared_tipset.key().into())),
        RpcTest::identity(ApiInfo::state_sector_get_info_req(
            shared_block.miner_address,
            101,
            shared_tipset.key().into(),
        )),
        RpcTest::identity(ApiInfo::state_miner_sectors_req(
            shared_block.miner_address,
            sectors,
            shared_tipset.key().into(),
        )),
        RpcTest::identity(ApiInfo::msig_get_available_balance_req(
            Address::new_id(18101), // msig address id
            shared_tipset.key().into(),
        )),
        RpcTest::identity(ApiInfo::msig_get_pending_req(
            Address::new_id(18101), // msig address id
            shared_tipset.key().into(),
        )),
        RpcTest::identity_raw(
            StateGetBeaconEntry::request((shared_tipset.epoch().into(),)).unwrap(),
        ),
    ]
}

fn wallet_tests() -> Vec<RpcTest> {
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

    vec![
        RpcTest::identity_raw(WalletBalance::request((known_wallet.into(),)).unwrap()),
        RpcTest::identity_raw(WalletValidateAddress::request((known_wallet.to_string(),)).unwrap()),
        RpcTest::identity_raw(
            WalletVerify::request((known_wallet.into(), text.into(), signature.into())).unwrap(),
        ),
        // These methods require write access in Lotus. Not sure why.
        // RpcTest::basic(ApiInfo::wallet_default_address_req()),
        // RpcTest::basic(ApiInfo::wallet_list_req()),
        // RpcTest::basic(ApiInfo::wallet_has_req(known_wallet.to_string())),
    ]
}

fn eth_tests() -> Vec<RpcTest> {
    vec![
        RpcTest::identity(ApiInfo::eth_accounts_req()),
        RpcTest::basic(ApiInfo::eth_block_number_req()),
        RpcTest::identity(ApiInfo::eth_chain_id_req()),
        // There is randomness in the result of this API
        RpcTest::basic(ApiInfo::eth_gas_price_req()),
        RpcTest::basic(ApiInfo::eth_syncing_req()),
        RpcTest::identity(ApiInfo::eth_get_balance_req(
            EthAddress::from_str("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa").unwrap(),
            BlockNumberOrHash::from_predefined(Predefined::Latest),
        )),
        RpcTest::identity(ApiInfo::eth_get_balance_req(
            EthAddress::from_str("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa").unwrap(),
            BlockNumberOrHash::from_predefined(Predefined::Pending),
        )),
        RpcTest::basic(ApiInfo::web3_client_version_req()),
    ]
}

fn eth_tests_with_tipset(shared_tipset: &Tipset) -> Vec<RpcTest> {
    vec![
        RpcTest::identity(ApiInfo::eth_get_balance_req(
            EthAddress::from_str("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa").unwrap(),
            BlockNumberOrHash::from_block_number(shared_tipset.epoch()),
        )),
        RpcTest::identity(ApiInfo::eth_get_balance_req(
            EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
            BlockNumberOrHash::from_block_number(shared_tipset.epoch()),
        )),
        RpcTest::identity(ApiInfo::eth_get_block_by_number_req(
            BlockNumberOrHash::from_block_number(shared_tipset.epoch()),
            false,
        )),
        RpcTest::identity(ApiInfo::eth_get_block_by_number_req(
            BlockNumberOrHash::from_block_number(shared_tipset.epoch()),
            true,
        )),
    ]
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
    vec![RpcTest::identity_raw(
        GasEstimateGasLimit::request((message.into(), LotusJson(shared_tipset.key().into())))
            .unwrap(),
    )]
}

// Extract tests that use chain-specific data such as block CIDs or message
// CIDs. Right now, only the last `n_tipsets` tipsets are used.
fn snapshot_tests(store: Arc<ManyCar>, n_tipsets: usize) -> anyhow::Result<Vec<RpcTest>> {
    let mut tests = vec![];
    // shared_tipset in the snapshot might not be finalized for the offline RPC server
    // use heaviest - 10 instead
    let shared_tipset = store
        .heaviest_tipset()?
        .chain(&store)
        .take(10)
        .last()
        .expect("Infallible");
    let shared_tipset_key = shared_tipset.key();
    tests.extend(chain_tests_with_tipset(&shared_tipset));
    tests.extend(state_tests_with_tipset(&shared_tipset));
    tests.extend(eth_tests_with_tipset(&shared_tipset));
    tests.extend(gas_tests_with_tipset(&shared_tipset));

    // Not easily verifiable by using addresses extracted from blocks as most of those yield `null`
    // for both Lotus and Forest. Therefore the actor addresses are hardcoded to values that allow
    // for API compatibility verification.
    tests.push(RpcTest::identity(ApiInfo::state_verified_client_status(
        Address::VERIFIED_REGISTRY_ACTOR,
        shared_tipset.key().into(),
    )));
    tests.push(RpcTest::identity(ApiInfo::state_verified_client_status(
        Address::DATACAP_TOKEN_ACTOR,
        shared_tipset.key().into(),
    )));

    for tipset in shared_tipset.clone().chain(&store).take(n_tipsets) {
        tests.push(RpcTest::identity_raw(ChainGetMessagesInTipset::request((
            tipset.key().clone().into(),
        ))?));
        tests.push(RpcTest::identity(
            ApiInfo::state_deal_provider_collateral_bounds_req(
                PaddedPieceSize(1),
                true,
                tipset.key().into(),
            ),
        ));
        tests.push(RpcTest::identity_raw(ChainTipSetWeight::request((
            LotusJson(tipset.key().into()),
        ))?));
        for block in tipset.block_headers() {
            let block_cid = (*block.cid()).into();
            tests.extend([
                RpcTest::identity_raw(ChainGetBlockMessages::request((block_cid,))?),
                RpcTest::identity_raw(ChainGetParentMessages::request((block_cid,))?),
                RpcTest::identity_raw(ChainGetParentReceipts::request((block_cid,))?),
            ]);
            tests.push(RpcTest::identity(ApiInfo::state_miner_active_sectors_req(
                block.miner_address,
                shared_tipset_key.into(),
            )));

            let (bls_messages, secp_messages) = crate::chain::store::block_messages(&store, block)?;
            for msg in bls_messages.into_iter().unique() {
                tests.push(RpcTest::identity_raw(ChainGetMessage::request((msg
                    .cid()?
                    .into(),))?));
                tests.push(RpcTest::identity(ApiInfo::state_account_key_req(
                    msg.from(),
                    shared_tipset_key.into(),
                )));
                tests.push(RpcTest::identity(ApiInfo::state_account_key_req(
                    msg.from(),
                    Default::default(),
                )));
                tests.push(RpcTest::identity(ApiInfo::state_lookup_id_req(
                    msg.from(),
                    shared_tipset_key.into(),
                )));
                tests.push(
                    validate_message_lookup(ApiInfo::state_wait_msg_req(msg.cid()?, 0))
                        .with_timeout(Duration::from_secs(30)),
                );
                tests.push(validate_message_lookup(ApiInfo::state_search_msg_req(
                    msg.cid()?,
                )));
                tests.push(validate_message_lookup(
                    ApiInfo::state_search_msg_limited_req(msg.cid()?, 800),
                ));
                tests.push(RpcTest::identity(ApiInfo::state_list_messages_req(
                    MessageFilter {
                        from: Some(msg.from()),
                        to: Some(msg.to()),
                    },
                    shared_tipset_key.into(),
                    shared_tipset.epoch(),
                )));
                tests.push(validate_message_lookup(ApiInfo::state_search_msg_req(
                    msg.cid()?,
                )));
                tests.push(validate_message_lookup(
                    ApiInfo::state_search_msg_limited_req(msg.cid()?, 800),
                ));
            }
            for msg in secp_messages.into_iter().unique() {
                tests.push(RpcTest::identity_raw(ChainGetMessage::request((msg
                    .cid()?
                    .into(),))?));
                tests.push(RpcTest::identity(ApiInfo::state_account_key_req(
                    msg.from(),
                    shared_tipset_key.into(),
                )));
                tests.push(RpcTest::identity(ApiInfo::state_account_key_req(
                    msg.from(),
                    Default::default(),
                )));
                tests.push(RpcTest::identity(ApiInfo::state_lookup_id_req(
                    msg.from(),
                    shared_tipset_key.into(),
                )));
                tests.push(
                    validate_message_lookup(ApiInfo::state_wait_msg_req(msg.cid()?, 0))
                        .with_timeout(Duration::from_secs(30)),
                );
                tests.push(validate_message_lookup(ApiInfo::state_search_msg_req(
                    msg.cid()?,
                )));
                tests.push(validate_message_lookup(
                    ApiInfo::state_search_msg_limited_req(msg.cid()?, 800),
                ));
                tests.push(RpcTest::basic(
                    MpoolGetNonce::request((msg.from().into(),)).unwrap(),
                ));
                tests.push(RpcTest::identity(ApiInfo::state_list_messages_req(
                    MessageFilter {
                        from: None,
                        to: Some(msg.to()),
                    },
                    shared_tipset_key.into(),
                    shared_tipset.epoch(),
                )));
                tests.push(RpcTest::identity(ApiInfo::state_list_messages_req(
                    MessageFilter {
                        from: Some(msg.from()),
                        to: None,
                    },
                    shared_tipset_key.into(),
                    shared_tipset.epoch(),
                )));
                tests.push(RpcTest::identity(ApiInfo::state_list_messages_req(
                    MessageFilter {
                        from: None,
                        to: None,
                    },
                    shared_tipset_key.into(),
                    shared_tipset.epoch(),
                )));

                if !msg.params().is_empty() {
                    tests.push(RpcTest::identity(ApiInfo::state_decode_params_req(
                            msg.to(),
                            msg.method_num(),
                            msg.params().to_vec(),
                            shared_tipset_key.into(),
                        )).ignore("Difficult to implement. Tracking issue: https://github.com/ChainSafe/forest/issues/3769"));
                }
            }
            tests.push(RpcTest::identity(ApiInfo::state_miner_info_req(
                block.miner_address,
                tipset.key().into(),
            )));
            tests.push(RpcTest::identity(ApiInfo::state_miner_power_req(
                block.miner_address,
                tipset.key().into(),
            )));
            tests.push(RpcTest::identity(ApiInfo::state_miner_deadlines_req(
                block.miner_address,
                tipset.key().into(),
            )));
            tests.push(RpcTest::identity(
                ApiInfo::state_miner_proving_deadline_req(block.miner_address, tipset.key().into()),
            ));
            tests.push(RpcTest::identity(
                ApiInfo::state_miner_available_balance_req(
                    block.miner_address,
                    tipset.key().into(),
                ),
            ));
            tests.push(RpcTest::identity(ApiInfo::state_miner_faults_req(
                block.miner_address,
                tipset.key().into(),
            )));
            tests.push(RpcTest::identity(ApiInfo::miner_get_base_info_req(
                block.miner_address,
                block.epoch,
                tipset.key().into(),
            )));
            tests.push(RpcTest::identity(ApiInfo::state_miner_recoveries_req(
                block.miner_address,
                tipset.key().into(),
            )));
            tests.push(RpcTest::identity(ApiInfo::state_miner_sector_count_req(
                block.miner_address,
                tipset.key().into(),
            )));
        }
        tests.push(RpcTest::identity(ApiInfo::state_circulating_supply_req(
            tipset.key().into(),
        )));
        tests.push(RpcTest::identity(
            ApiInfo::state_vm_circulating_supply_internal_req(tipset.key().into()),
        ));

        for block in tipset.block_headers() {
            let (bls_messages, secp_messages) = crate::chain::store::block_messages(&store, block)?;
            for msg in secp_messages {
                tests.push(RpcTest::identity(ApiInfo::state_call_req(
                    msg.message().clone(),
                    shared_tipset.key().into(),
                )));
            }
            for msg in bls_messages {
                tests.push(RpcTest::identity(ApiInfo::state_call_req(
                    msg.clone(),
                    shared_tipset.key().into(),
                )));
            }

            tests.push(RpcTest::identity(ApiInfo::state_market_balance_req(
                block.miner_address,
                tipset.key().into(),
            )));
        }

        // Get deals
        let deals = {
            let state = StateTree::new_from_root(store.clone(), tipset.parent_state())?;
            let actor = state
                .get_actor(&Address::MARKET_ACTOR)?
                .context("Market actor not found")?;
            let market_state = market::State::load(&store, actor.code, actor.state)?;
            let proposals = market_state.proposals(&store)?;
            let mut deals = vec![];
            proposals.for_each(|deal_id, _| {
                deals.push(deal_id);
                Ok(())
            })?;
            deals
        };

        // Take 5 deals from each tipset
        for deal in deals.into_iter().take(5) {
            tests.push(RpcTest::identity(ApiInfo::state_market_storage_deal_req(
                deal,
                tipset.key().into(),
            )));
        }
    }
    Ok(tests)
}

fn websocket_tests() -> Vec<RpcTest> {
    let test = RpcTest::identity(ApiInfo::chain_notify_req()).ignore("Not implemented yet");
    vec![test]
}

/// Compare two RPC providers. The providers are labeled `forest` and `lotus`,
/// but other nodes may be used (such as `venus`). The `lotus` node is assumed
/// to be correct and the `forest` node will be marked as incorrect if it
/// deviates.
///
/// If snapshot files are provided, these files will be used to generate
/// additional tests.
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
#[allow(clippy::too_many_arguments)]
async fn compare_apis(
    forest: ApiInfo,
    lotus: ApiInfo,
    snapshot_files: Vec<PathBuf>,
    config: ApiTestFlags,
) -> anyhow::Result<()> {
    let mut tests = vec![];

    tests.extend(common_tests());
    tests.extend(auth_tests());
    tests.extend(beacon_tests());
    tests.extend(chain_tests());
    tests.extend(mpool_tests());
    tests.extend(net_tests());
    tests.extend(node_tests());
    tests.extend(wallet_tests());
    tests.extend(eth_tests());
    tests.extend(state_tests());

    if !snapshot_files.is_empty() {
        let store = Arc::new(ManyCar::try_from(snapshot_files)?);
        tests.extend(snapshot_tests(store, config.n_tipsets)?);
    }

    if matches!(forest.scheme(), "ws" | "wss") && matches!(lotus.scheme(), "ws" | "wss") {
        tests.extend(websocket_tests())
    }

    tests.sort_by_key(|test| test.request.method_name);

    run_tests(tests, &forest, &lotus, &config).await
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

    let beacon = Arc::new(
        state_manager
            .chain_config()
            .get_beacon_schedule(chain_store.genesis_block_header().timestamp),
    );
    let (network_send, _) = flume::bounded(5);
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
        chain_store,
        beacon,
        shutdown,
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

async fn run_tests(
    tests: Vec<RpcTest>,
    forest: &ApiInfo,
    lotus: &ApiInfo,
    config: &ApiTestFlags,
) -> anyhow::Result<()> {
    let semaphore = Arc::new(Semaphore::new(config.max_concurrent_requests));
    let mut futures = FuturesUnordered::new();

    let filter_list = if let Some(filter_file) = &config.filter_file {
        FilterList::new_from_file(filter_file)?
    } else {
        FilterList::default().allow(config.filter.clone())
    };

    for test in tests.into_iter() {
        // By default, do not run ignored tests.
        if matches!(config.run_ignored, RunIgnored::Default) && test.ignore.is_some() {
            continue;
        }
        // If in `IgnoreOnly` mode, only run ignored tests.
        if matches!(config.run_ignored, RunIgnored::IgnoredOnly) && test.ignore.is_none() {
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
            (test.request.method_name, test_result)
        });

        futures.push(future);
    }

    let mut success_results = HashMap::default();
    let mut failed_results = HashMap::default();
    let mut fail_details = Vec::new();
    while let Some(Ok((method_name, test_result))) = futures.next().await {
        let forest_status = test_result.forest_status;
        let lotus_status = test_result.lotus_status;
        let result_entry = (method_name, forest_status, lotus_status);
        if (forest_status == EndpointStatus::Valid && lotus_status == EndpointStatus::Valid)
            || (forest_status == EndpointStatus::Timeout && lotus_status == EndpointStatus::Timeout)
        {
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

        if !failed_results.is_empty() && config.fail_fast {
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
    success_results: &HashMap<(&'static str, EndpointStatus, EndpointStatus), u32>,
    failed_results: &HashMap<(&'static str, EndpointStatus, EndpointStatus), u32>,
) {
    // Combine all results
    let mut combined_results = success_results.clone();
    for (key, value) in failed_results {
        combined_results.insert(*key, *value);
    }

    // Collect and display results in Markdown format
    let mut results = combined_results.into_iter().collect::<Vec<_>>();
    results.sort();
    println!("{}", format_as_markdown(&results));
}

fn format_as_markdown(results: &[((&'static str, EndpointStatus, EndpointStatus), u32)]) -> String {
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

fn validate_message_lookup(req: RpcRequest<Option<MessageLookup>>) -> RpcTest {
    use libipld_core::ipld::Ipld;

    RpcTest::validate(req, |mut forest, mut lotus| {
        // TODO(hanabi1224): https://github.com/ChainSafe/forest/issues/3784
        if let Some(json) = forest.as_mut() {
            json.return_dec = Ipld::Null;
        }
        if let Some(json) = lotus.as_mut() {
            json.return_dec = Ipld::Null;
        }
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
