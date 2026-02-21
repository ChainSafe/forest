// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{CreateTestsArgs, ReportMode, RunIgnored, TestCriteriaOverride};
use crate::blocks::{ElectionProof, Ticket, Tipset};
use crate::chain::ChainStore;
use crate::db::car::ManyCar;
use crate::eth::{EthChainId as EthChainIdType, SAFE_EPOCH_DELAY};
use crate::lotus_json::HasLotusJson;
use crate::message::{Message as _, SignedMessage};
use crate::rpc::FilterList;
use crate::rpc::auth::AuthNewParams;
use crate::rpc::beacon::BeaconGetEntry;
use crate::rpc::eth::{
    BlockNumberOrHash, EthInt64, ExtBlockNumberOrHash, ExtPredefined, Predefined,
    new_eth_tx_from_signed_message, types::*,
};
use crate::rpc::gas::{GasEstimateGasLimit, GasEstimateMessageGas};
use crate::rpc::miner::BlockTemplate;
use crate::rpc::misc::ActorEventFilter;
use crate::rpc::state::StateGetAllClaims;
use crate::rpc::types::*;
use crate::rpc::{Permission, prelude::*};
use crate::shim::actors::MarketActorStateLoad as _;
use crate::shim::actors::market;
use crate::shim::executor::Receipt;
use crate::shim::sector::SectorSize;
use crate::shim::{
    address::{Address, Protocol},
    crypto::Signature,
    econ::TokenAmount,
    message::{METHOD_SEND, Message},
    state_tree::StateTree,
};
use crate::state_manager::StateManager;
use crate::tool::offline_server::server::handle_chain_config;
use crate::tool::subcommands::api_cmd::NetworkChain;
use crate::tool::subcommands::api_cmd::report::ReportBuilder;
use crate::tool::subcommands::api_cmd::state_decode_params_tests::create_all_state_decode_params_tests;
use crate::utils::proofs_api::{self, ensure_proof_params_downloaded};
use crate::{Config, rpc};
use ahash::HashMap;
use bls_signatures::Serialize as _;
use chrono::Utc;
use cid::Cid;
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use fil_actors_shared::v10::runtime::DomainSeparationTag;
use fvm_ipld_blockstore::Blockstore;
use ipld_core::ipld::Ipld;
use itertools::Itertools as _;
use jsonrpsee::types::ErrorCode;
use libp2p::PeerId;
use libsecp256k1::{PublicKey, SecretKey};
use num_traits::Signed;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use similar::{ChangeTag, TextDiff};
use std::borrow::Cow;
use std::path::Path;
use std::time::Instant;
use std::{
    path::PathBuf,
    str::FromStr,
    sync::{Arc, LazyLock},
    time::Duration,
};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::debug;

const COLLECTION_SAMPLE_SIZE: usize = 5;

/// This address has been funded by the calibnet faucet and the private keys
/// has been discarded. It should always have a non-zero balance.
static KNOWN_CALIBNET_ADDRESS: LazyLock<Address> = LazyLock::new(|| {
    crate::shim::address::Network::Testnet
        .parse_address("t1c4dkec3qhrnrsa4mccy7qntkyq2hhsma4sq7lui")
        .unwrap()
        .into()
});

/// This address is known to be empty on calibnet. It should always have a zero balance.
static KNOWN_EMPTY_CALIBNET_ADDRESS: LazyLock<Address> = LazyLock::new(|| {
    crate::shim::address::Network::Testnet
        .parse_address("t1qb2x5qctp34rxd7ucl327h5ru6aazj2heno7x5y")
        .unwrap()
        .into()
});

// this is the ID address of the `t1w2zb5a723izlm4q3khclsjcnapfzxcfhvqyfoly` address
static KNOWN_CALIBNET_F0_ADDRESS: LazyLock<Address> = LazyLock::new(|| {
    crate::shim::address::Network::Testnet
        .parse_address("t0168923")
        .unwrap()
        .into()
});

static KNOWN_CALIBNET_F1_ADDRESS: LazyLock<Address> = LazyLock::new(|| {
    crate::shim::address::Network::Testnet
        .parse_address("t1w2zb5a723izlm4q3khclsjcnapfzxcfhvqyfoly")
        .unwrap()
        .into()
});

static KNOWN_CALIBNET_F2_ADDRESS: LazyLock<Address> = LazyLock::new(|| {
    crate::shim::address::Network::Testnet
        .parse_address("t2nfplhzpyeck5dcc4fokj5ar6nbs3mhbdmq6xu3q")
        .unwrap()
        .into()
});

static KNOWN_CALIBNET_F3_ADDRESS: LazyLock<Address> = LazyLock::new(|| {
    crate::shim::address::Network::Testnet
        .parse_address("t3wmbvnabsj6x2uki33phgtqqemmunnttowpx3chklrchy76pv52g5ajnaqdypxoomq5ubfk65twl5ofvkhshq")
        .unwrap()
        .into()
});

static KNOWN_CALIBNET_F4_ADDRESS: LazyLock<Address> = LazyLock::new(|| {
    crate::shim::address::Network::Testnet
        .parse_address("t410fx2cumi6pgaz64varl77xbuub54bgs3k5xsvn3ki")
        .unwrap()
        .into()
});

fn generate_eth_random_address() -> anyhow::Result<EthAddress> {
    let rng = &mut crate::utils::rand::forest_os_rng();
    let secret_key = SecretKey::random(rng);
    let public_key = PublicKey::from_secret_key(&secret_key);
    EthAddress::eth_address_from_pub_key(&public_key.serialize())
}

const TICKET_QUALITY_GREEDY: f64 = 0.9;
const TICKET_QUALITY_OPTIMAL: f64 = 0.8;
const ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";
// miner actor address `t078216`
const MINER_ADDRESS: Address = Address::new_id(78216); // https://calibration.filscan.io/en/miner/t078216
const ACCOUNT_ADDRESS: Address = Address::new_id(1234); // account actor address `t01234`
const EVM_ADDRESS: &str = "t410fbqoynu2oi2lxam43knqt6ordiowm2ywlml27z4i";

/// Brief description of a single method call against a single host
#[derive(
    Debug, Clone, PartialOrd, Ord, PartialEq, Eq, Hash, Serialize, Deserialize, strum::Display,
)]
#[serde(rename_all = "snake_case")]
pub enum TestSummary {
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
    /// Server returned JSON-RPC, and it matched our schema, but failed validation
    CustomCheckFailed,
    /// Server timed out
    Timeout,
    /// Server returned JSON-RPC, and it matched our schema, and passed validation
    Valid,
}

impl TestSummary {
    fn from_err(err: &rpc::ClientError) -> Self {
        match err {
            rpc::ClientError::Call(it) => match it.code().into() {
                ErrorCode::MethodNotFound => Self::MissingMethod,
                _ => {
                    // `lotus-gateway` adds `RPC error (-32603):` prefix to the error message that breaks tests,
                    // normalize the error message first
                    let message = normalized_error_message(it.message());
                    Self::Rejected(message.to_string())
                }
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
            _ => unimplemented!(),
        }
    }
}

/// Data about a failed test. Used for debugging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestDump {
    pub request: rpc::Request,
    pub path: rpc::ApiPaths,
    pub forest_response: Result<Value, String>,
    pub lotus_response: Result<Value, String>,
}

impl std::fmt::Display for TestDump {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Request path: {}", self.path.path())?;
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
        if let Some(forest_response) = &forest_response
            && let Some(lotus_response) = &lotus_response
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
            writeln!(f, "Forest response: {forest_response}")?;
            writeln!(f, "Lotus response: {lotus_response}")?;
            writeln!(f, "Diff: {}", print_diff.join("\n"))?;
        } else {
            if let Some(forest_response) = &forest_response {
                writeln!(f, "Forest response: {forest_response}")?;
            }
            if let Some(lotus_response) = &lotus_response {
                writeln!(f, "Lotus response: {lotus_response}")?;
            }
        };
        Ok(())
    }
}

/// Result of running a single RPC test
pub struct TestResult {
    /// Forest result after calling the RPC method.
    pub forest_status: TestSummary,
    /// Lotus result after calling the RPC method.
    pub lotus_status: TestSummary,
    /// Optional data dump if either status was invalid.
    pub test_dump: Option<TestDump>,
    /// Duration of the RPC call.
    pub duration: Duration,
}

pub(super) enum PolicyOnRejected {
    Fail,
    Pass,
    PassWithIdenticalError,
    PassWithIdenticalErrorCaseInsensitive,
    /// If Forest reason is a subset of Lotus reason, the test passes.
    /// We don't always bubble up errors and format the error chain like Lotus.
    PassWithQuasiIdenticalError,
}

pub(super) enum SortPolicy {
    /// Recursively sorts both arrays and maps in a JSON value.
    All,
}

pub(super) struct RpcTest {
    pub request: rpc::Request,
    pub check_syntax: Arc<dyn Fn(serde_json::Value) -> bool + Send + Sync>,
    pub check_semantics: Arc<dyn Fn(serde_json::Value, serde_json::Value) -> bool + Send + Sync>,
    pub ignore: Option<&'static str>,
    pub policy_on_rejected: PolicyOnRejected,
    pub sort_policy: Option<SortPolicy>,
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
    pub(crate) fn identity<T: PartialEq + HasLotusJson>(request: rpc::Request<T>) -> RpcTest {
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
        let start = Instant::now();
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
                path: self.request.api_path().expect("invalid api paths"),
                forest_response,
                lotus_response,
            }),
            duration: start.elapsed(),
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
    offline: bool,
    tipset: &Tipset,
) -> anyhow::Result<Vec<RpcTest>> {
    let mut tests = vec![
        RpcTest::identity(ChainGetTipSetByHeight::request((
            tipset.epoch(),
            Default::default(),
        ))?),
        RpcTest::identity(ChainGetTipSetAfterHeight::request((
            tipset.epoch(),
            Default::default(),
        ))?),
        RpcTest::identity(ChainGetTipSet::request((tipset.key().into(),))?),
        RpcTest::identity(ChainGetTipSet::request((None.into(),))?)
            .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::identity(ChainGetTipSetV2::request((TipsetSelector {
            key: None.into(),
            height: None,
            tag: None,
        },))?)
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::identity(ChainGetTipSetV2::request((TipsetSelector {
            key: tipset.key().into(),
            height: None,
            tag: Some(TipsetTag::Latest),
        },))?)
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::identity(ChainGetTipSetV2::request((TipsetSelector {
            key: tipset.key().into(),
            height: None,
            tag: None,
        },))?),
        RpcTest::identity(ChainGetTipSetV2::request((TipsetSelector {
            key: None.into(),
            height: Some(TipsetHeight {
                at: tipset.epoch(),
                previous: true,
                anchor: Some(TipsetAnchor {
                    key: None.into(),
                    tag: None,
                }),
            }),
            tag: None,
        },))?),
        RpcTest::identity(ChainGetTipSetV2::request((TipsetSelector {
            key: None.into(),
            height: Some(TipsetHeight {
                at: tipset.epoch(),
                previous: true,
                anchor: None,
            }),
            tag: None,
        },))?)
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError)
        .ignore("this case should pass when F3 is back on calibnet"),
        validate_tagged_tipset_v2(
            ChainGetTipSetV2::request((TipsetSelector {
                key: None.into(),
                height: None,
                tag: Some(TipsetTag::Latest),
            },))?,
            offline,
        ),
        RpcTest::identity(ChainGetPath::request((
            tipset.key().clone(),
            tipset.parents().clone(),
        ))?),
        RpcTest::identity(ChainGetMessagesInTipset::request((tipset
            .key()
            .clone()
            .into(),))?),
        RpcTest::identity(ChainTipSetWeight::request((tipset.key().into(),))?),
        RpcTest::basic(ChainGetFinalizedTipset::request(())?),
    ];

    if !offline {
        tests.extend([
            // Requires F3, disabled for offline RPC server
            validate_tagged_tipset_v2(
                ChainGetTipSetV2::request((TipsetSelector {
                    key: None.into(),
                    height: None,
                    tag: Some(TipsetTag::Safe),
                },))?,
                offline,
            ),
            // Requires F3, disabled for offline RPC server
            validate_tagged_tipset_v2(
                ChainGetTipSetV2::request((TipsetSelector {
                    key: None.into(),
                    height: None,
                    tag: Some(TipsetTag::Finalized),
                },))?,
                offline,
            ),
        ]);
    }

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

        for receipt in Receipt::get_receipts(store, block.message_receipts)? {
            if let Some(events_root) = receipt.events_root() {
                tests.extend([RpcTest::identity(ChainGetEvents::request((events_root,))?)
                    .sort_policy(SortPolicy::All)]);
            }
        }
    }

    Ok(tests)
}

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
        RpcTest::identity(MpoolGetNonce::request((*KNOWN_CALIBNET_ADDRESS,)).unwrap()),
        // This should cause an error with `actor not found` in both Lotus and Forest. The messages
        // are quite different, so we don't do strict equality check.
        //  "forest_response": {
        //    "Err": "ErrorObject { code: InternalError, message: \"Actor not found: addr=t1qb2x5qctp34rxd7ucl327h5ru6aazj2heno7x5y\", data: None }"
        //  },
        //  "lotus_response": {
        //    "Err": "ErrorObject { code: ServerError(1), message: \"resolution lookup failed (t1qb2x5qctp34rxd7ucl327h5ru6aazj2heno7x5y): resolve address t1qb2x5qctp34rxd7ucl327h5ru6aazj2heno7x5y: actor not found\", data: None }"
        //  }
        RpcTest::identity(MpoolGetNonce::request((*KNOWN_EMPTY_CALIBNET_ADDRESS,)).unwrap())
            .policy_on_rejected(PolicyOnRejected::Pass),
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

fn event_tests_with_tipset<DB: Blockstore>(_store: &Arc<DB>, tipset: &Tipset) -> Vec<RpcTest> {
    let epoch = tipset.epoch();
    vec![
        RpcTest::identity(GetActorEventsRaw::request((None,)).unwrap())
            .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::identity(
            GetActorEventsRaw::request((Some(ActorEventFilter {
                addresses: vec![],
                fields: Default::default(),
                from_height: Some(epoch),
                to_height: Some(epoch),
                tipset_key: None,
            }),))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError)
        .sort_policy(SortPolicy::All),
        RpcTest::identity(
            GetActorEventsRaw::request((Some(ActorEventFilter {
                addresses: vec![],
                fields: Default::default(),
                from_height: Some(epoch - 100),
                to_height: Some(epoch),
                tipset_key: None,
            }),))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError)
        .sort_policy(SortPolicy::All),
        RpcTest::identity(
            GetActorEventsRaw::request((Some(ActorEventFilter {
                addresses: vec![],
                fields: Default::default(),
                from_height: None,
                to_height: None,
                tipset_key: Some(tipset.key().clone().into()),
            }),))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError)
        .sort_policy(SortPolicy::All),
        RpcTest::identity(
            GetActorEventsRaw::request((Some(ActorEventFilter {
                addresses: vec![
                    Address::from_str("t410fvtakbtytk4otbnfymn4zn5ow252nj7lcpbtersq")
                        .unwrap()
                        .into(),
                ],
                fields: Default::default(),
                from_height: Some(epoch - 100),
                to_height: Some(epoch),
                tipset_key: None,
            }),))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError)
        .sort_policy(SortPolicy::All),
        {
            use std::collections::BTreeMap;

            use base64::{Engine, prelude::BASE64_STANDARD};

            use crate::lotus_json::LotusJson;
            use crate::rpc::misc::ActorEventBlock;

            let topic = BASE64_STANDARD
                .decode("0Gprf0kYSUs3GSF9GAJ4bB9REqbB2I/iz+wAtFhPauw=")
                .unwrap();
            let mut fields: BTreeMap<String, Vec<ActorEventBlock>> = Default::default();
            fields.insert(
                "t1".into(),
                vec![ActorEventBlock {
                    codec: 85,
                    value: LotusJson(topic),
                }],
            );
            RpcTest::identity(
                GetActorEventsRaw::request((Some(ActorEventFilter {
                    addresses: vec![],
                    fields,
                    from_height: Some(epoch - 100),
                    to_height: Some(epoch),
                    tipset_key: None,
                }),))
                .unwrap(),
            )
            .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError)
            .sort_policy(SortPolicy::All)
        },
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
    let priv_key = bls_signatures::PrivateKey::generate(&mut crate::utils::rand::forest_rng());
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
        RpcTest::identity(StateGetActorV2::request((
            Address::SYSTEM_ACTOR,
            TipsetSelector {
                key: tipset.key().into(),
                ..Default::default()
            },
        ))?),
        RpcTest::identity(StateGetID::request((
            Address::SYSTEM_ACTOR,
            TipsetSelector {
                key: tipset.key().into(),
                ..Default::default()
            },
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
            u16::MAX as _,      // invalid sector number
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
        RpcTest::identity(StateCompute::request((
            tipset.epoch(),
            vec![],
            tipset.key().into(),
        ))?),
    ];

    tests.extend(read_state_api_tests(tipset)?);
    tests.extend(create_all_state_decode_params_tests(tipset)?);

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
    let prefunded_wallets = [
        // the following addresses should have 666 attoFIL each
        *KNOWN_CALIBNET_F0_ADDRESS,
        *KNOWN_CALIBNET_F1_ADDRESS,
        *KNOWN_CALIBNET_F2_ADDRESS,
        *KNOWN_CALIBNET_F3_ADDRESS,
        *KNOWN_CALIBNET_F4_ADDRESS,
        // This address should have 0 FIL
        *KNOWN_EMPTY_CALIBNET_ADDRESS,
    ];

    let mut tests = vec![];
    for wallet in prefunded_wallets {
        tests.push(RpcTest::identity(
            WalletBalance::request((wallet,)).unwrap(),
        ));
        tests.push(RpcTest::identity(
            WalletValidateAddress::request((wallet.to_string(),)).unwrap(),
        ));
    }

    let known_wallet = *KNOWN_CALIBNET_ADDRESS;
    // "Hello world!" signed with the above address:
    let signature = "44364ca78d85e53dda5ac6f719a4f2de3261c17f58558ab7730f80c478e6d43775244e7d6855afad82e4a1fd6449490acfa88e3fcfe7c1fe96ed549c100900b400";
    let text = "Hello world!".as_bytes().to_vec();
    let sig_bytes = hex::decode(signature).unwrap();
    let signature = match known_wallet.protocol() {
        Protocol::Secp256k1 => Signature::new_secp256k1(sig_bytes),
        Protocol::BLS => Signature::new_bls(sig_bytes),
        _ => panic!("Invalid signature (must be bls or secp256k1)"),
    };

    tests.push(RpcTest::identity(
        WalletBalance::request((known_wallet,)).unwrap(),
    ));
    tests.push(RpcTest::identity(
        WalletValidateAddress::request((known_wallet.to_string(),)).unwrap(),
    ));
    tests.push(
        RpcTest::identity(
            // Both Forest and Lotus should fail miserably at invocking Cthulhu's name
            WalletValidateAddress::request((
                "Ph'nglui mglw'nafh Cthulhu R'lyeh wgah'nagl fhtagn".to_string(),
            ))
            .unwrap(),
        )
        // Forest returns `Unknown address network`, Lotus `unknown address network`.
        .policy_on_rejected(PolicyOnRejected::PassWithIdenticalErrorCaseInsensitive),
    );
    tests.push(RpcTest::identity(
        WalletVerify::request((known_wallet, text, signature)).unwrap(),
    ));

    // If a worker address is provided, we can test wallet methods requiring
    // a shared key.
    if let Some(worker_address) = worker_address {
        use base64::{Engine, prelude::BASE64_STANDARD};
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
        // There is randomness in the result of this API, but at least check that the results are non-zero.
        tests.push(RpcTest::validate(
            EthGasPrice::request_with_alias((), use_alias).unwrap(),
            |forest, lotus| forest.0.is_positive() && lotus.0.is_positive(),
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

        let cases = [
            (
                Some(EthAddress::from_str("0x0c1d86d34e469770339b53613f3a2343accd62cb").unwrap()),
                Some(
                    "0xf8b2cb4f000000000000000000000000CbfF24DED1CE6B53712078759233Ac8f91ea71B6"
                        .parse()
                        .unwrap(),
                ),
            ),
            (Some(EthAddress::from_str(ZERO_ADDRESS).unwrap()), None),
            // Assert contract creation, which is invoked via setting the `to` field to `None` and
            // providing the contract bytecode in the `data` field.
            (
                None,
                Some(
                    EthBytes::from_str(
                        concat!("0x", include_str!("contracts/cthulhu/invoke.hex")).trim(),
                    )
                    .unwrap(),
                ),
            ),
        ];

        for (to, data) in cases {
            let msg = EthCallMessage {
                to,
                data: data.clone(),
                ..EthCallMessage::default()
            };

            tests.push(RpcTest::identity(
                EthCall::request_with_alias(
                    (
                        msg.clone(),
                        BlockNumberOrHash::from_predefined(Predefined::Latest),
                    ),
                    use_alias,
                )
                .unwrap(),
            ));

            for tag in [
                ExtPredefined::Latest,
                ExtPredefined::Safe,
                ExtPredefined::Finalized,
            ] {
                tests.push(RpcTest::identity(
                    EthCallV2::request_with_alias(
                        (msg.clone(), ExtBlockNumberOrHash::PredefinedBlock(tag)),
                        use_alias,
                    )
                    .unwrap(),
                ));
            }
        }

        let cases = [
            Some(EthAddressList::List(vec![])),
            Some(EthAddressList::List(vec![
                EthAddress::from_str("0x0c1d86d34e469770339b53613f3a2343accd62cb").unwrap(),
                EthAddress::from_str("0x89beb26addec4bc7e9f475aacfd084300d6de719").unwrap(),
            ])),
            Some(EthAddressList::Single(
                EthAddress::from_str("0x0c1d86d34e469770339b53613f3a2343accd62cb").unwrap(),
            )),
            None,
        ];

        for address in cases {
            tests.push(RpcTest::basic(
                EthNewFilter::request_with_alias(
                    (EthFilterSpec {
                        address,
                        ..Default::default()
                    },),
                    use_alias,
                )
                .unwrap(),
            ));
        }
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
            EthAddressToFilecoinAddress::request(("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa"
                .parse()
                .unwrap(),))
            .unwrap(),
        ));
        tests.push(RpcTest::identity(
            FilecoinAddressToEthAddress::request((*KNOWN_CALIBNET_F0_ADDRESS, None)).unwrap(),
        ));
        tests.push(RpcTest::identity(
            FilecoinAddressToEthAddress::request((*KNOWN_CALIBNET_F1_ADDRESS, None)).unwrap(),
        ));
        tests.push(RpcTest::identity(
            FilecoinAddressToEthAddress::request((*KNOWN_CALIBNET_F2_ADDRESS, None)).unwrap(),
        ));
        tests.push(RpcTest::identity(
            FilecoinAddressToEthAddress::request((*KNOWN_CALIBNET_F3_ADDRESS, None)).unwrap(),
        ));
        tests.push(RpcTest::identity(
            FilecoinAddressToEthAddress::request((*KNOWN_CALIBNET_F4_ADDRESS, None)).unwrap(),
        ));
    }
    tests
}

fn eth_call_api_err_tests(epoch: i64) -> Vec<RpcTest> {
    let contract_codes = [
        include_str!("./contracts/arithmetic_err/arithmetic_overflow_err.hex"),
        include_str!("contracts/assert_err/assert_err.hex"),
        include_str!("./contracts/divide_by_zero_err/divide_by_zero_err.hex"),
        include_str!("./contracts/generic_panic_err/generic_panic_err.hex"),
        include_str!("./contracts/index_out_of_bounds_err/index_out_of_bounds_err.hex"),
        include_str!("./contracts/invalid_enum_err/invalid_enum_err.hex"),
        include_str!("./contracts/invalid_storage_array_err/invalid_storage_array_err.hex"),
        include_str!("./contracts/out_of_memory_err/out_of_memory_err.hex"),
        include_str!("./contracts/pop_empty_array_err/pop_empty_array_err.hex"),
        include_str!("./contracts/uninitialized_fn_err/uninitialized_fn_err.hex"),
    ];

    let mut tests = Vec::new();

    for &contract_hex in &contract_codes {
        let contract_code =
            EthBytes::from_str(contract_hex).expect("Contract bytecode should be valid hex");

        let zero_address = EthAddress::from_str(ZERO_ADDRESS).unwrap();
        // Setting the `EthCallMessage` `to` field to null will deploy the contract.
        let msg = EthCallMessage {
            from: Some(zero_address),
            data: Some(contract_code),
            ..EthCallMessage::default()
        };

        let eth_call_request =
            EthCall::request((msg.clone(), BlockNumberOrHash::from_block_number(epoch))).unwrap();

        tests.push(
            RpcTest::identity(eth_call_request)
                .policy_on_rejected(PolicyOnRejected::PassWithIdenticalError),
        );

        let eth_call_v2_request =
            EthCallV2::request((msg, ExtBlockNumberOrHash::from_block_number(epoch))).unwrap();

        tests.push(
            RpcTest::identity(eth_call_v2_request)
                .policy_on_rejected(PolicyOnRejected::PassWithIdenticalError),
        );
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
                BlockNumberOrHash::from_block_hash_object(block_hash, false),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBalance::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                BlockNumberOrHash::from_block_hash_object(block_hash, true),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBalance::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                BlockNumberOrHash::from_predefined(Predefined::Earliest),
            ))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::basic(
            EthGetBalance::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                BlockNumberOrHash::from_predefined(Predefined::Pending),
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetBalance::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                BlockNumberOrHash::from_predefined(Predefined::Latest),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBalance::request((
                generate_eth_random_address().unwrap(),
                BlockNumberOrHash::from_predefined(Predefined::Latest),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBalanceV2::request((
                EthAddress::from_str("0xff38c072f286e3b20b3954ca9f99c05fbecc64aa").unwrap(),
                ExtBlockNumberOrHash::from_block_number(shared_tipset.epoch()),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBalanceV2::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                ExtBlockNumberOrHash::from_block_number(shared_tipset.epoch()),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBalanceV2::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                ExtBlockNumberOrHash::from_block_number_object(shared_tipset.epoch()),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBalanceV2::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                ExtBlockNumberOrHash::from_block_hash_object(block_hash, false),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBalanceV2::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                ExtBlockNumberOrHash::from_block_hash_object(block_hash, true),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBalanceV2::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Earliest),
            ))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::basic(
            EthGetBalanceV2::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Pending),
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetBalanceV2::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Latest),
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetBalanceV2::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Safe),
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetBalanceV2::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Finalized),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBalanceV2::request((
                generate_eth_random_address().unwrap(),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Latest),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBlockByNumber::request((
                BlockNumberOrPredefined::BlockNumber(EthInt64(shared_tipset.epoch())),
                false,
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBlockByNumber::request((
                BlockNumberOrPredefined::BlockNumber(EthInt64(shared_tipset.epoch())),
                true,
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBlockByNumber::request((
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Earliest),
                true,
            ))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::basic(
            EthGetBlockByNumber::request((
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Pending),
                true,
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetBlockByNumber::request((
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Latest),
                true,
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetBlockByNumber::request((
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Safe),
                true,
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetBlockByNumber::request((
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Finalized),
                true,
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBlockByNumberV2::request((
                BlockNumberOrPredefined::BlockNumber(EthInt64(shared_tipset.epoch())),
                false,
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBlockByNumberV2::request((
                BlockNumberOrPredefined::BlockNumber(EthInt64(shared_tipset.epoch())),
                true,
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBlockByNumberV2::request((
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Earliest),
                true,
            ))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::basic(
            EthGetBlockByNumberV2::request((
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Pending),
                true,
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetBlockByNumberV2::request((
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Latest),
                true,
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetBlockByNumberV2::request((
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Safe),
                true,
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetBlockByNumberV2::request((
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Finalized),
                true,
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBlockReceipts::request((BlockNumberOrHash::from_block_hash_object(
                block_hash, true,
            ),))
            .unwrap(),
        ),
        // Nodes might be synced to different epochs, so we can't assert the exact result here.
        // Regardless, we want to check if the node returns a valid response and accepts predefined
        // values.
        RpcTest::basic(
            EthGetBlockReceipts::request((BlockNumberOrHash::from_predefined(Predefined::Latest),))
                .unwrap(),
        ),
        RpcTest::identity(
            EthGetBlockReceiptsV2::request((ExtBlockNumberOrHash::from_block_hash_object(
                block_hash, true,
            ),))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetBlockReceiptsV2::request((ExtBlockNumberOrHash::from_predefined(
                ExtPredefined::Safe,
            ),))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetBlockReceiptsV2::request((ExtBlockNumberOrHash::from_predefined(
                ExtPredefined::Finalized,
            ),))
            .unwrap(),
        ),
        RpcTest::identity(EthGetBlockTransactionCountByHash::request((block_hash,)).unwrap()),
        RpcTest::identity(
            EthGetBlockReceiptsLimited::request((
                BlockNumberOrHash::from_block_hash_object(block_hash, true),
                4,
            ))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithIdenticalError),
        RpcTest::identity(
            EthGetBlockReceiptsLimited::request((
                BlockNumberOrHash::from_block_hash_object(block_hash, true),
                -1,
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBlockReceiptsLimitedV2::request((
                ExtBlockNumberOrHash::from_block_hash_object(block_hash, true),
                4,
            ))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithIdenticalError),
        RpcTest::identity(
            EthGetBlockReceiptsLimitedV2::request((
                ExtBlockNumberOrHash::from_block_hash_object(block_hash, true),
                -1,
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBlockTransactionCountByNumber::request((EthInt64(shared_tipset.epoch()),))
                .unwrap(),
        ),
        RpcTest::identity(
            EthGetBlockTransactionCountByNumberV2::request((BlockNumberOrPredefined::BlockNumber(
                EthInt64(shared_tipset.epoch()),
            ),))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBlockTransactionCountByNumberV2::request((
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Safe),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetBlockTransactionCountByNumberV2::request((
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Finalized),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetTransactionCount::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                BlockNumberOrHash::from_block_hash_object(block_hash, true),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetTransactionCount::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                BlockNumberOrHash::from_predefined(Predefined::Earliest),
            ))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::basic(
            EthGetTransactionCount::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                BlockNumberOrHash::from_predefined(Predefined::Pending),
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetTransactionCount::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                BlockNumberOrHash::from_predefined(Predefined::Latest),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetTransactionCount::request((
                generate_eth_random_address().unwrap(),
                BlockNumberOrHash::from_predefined(Predefined::Latest),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetTransactionCountV2::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                ExtBlockNumberOrHash::from_block_hash_object(block_hash, true),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetTransactionCountV2::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Earliest),
            ))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::basic(
            EthGetTransactionCountV2::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Pending),
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetTransactionCountV2::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Latest),
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetTransactionCountV2::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Safe),
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetTransactionCountV2::request((
                EthAddress::from_str("0xff000000000000000000000000000000000003ec").unwrap(),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Finalized),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetTransactionCountV2::request((
                generate_eth_random_address().unwrap(),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Latest),
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
            EthGetStorageAt::request((
                EthAddress::from_str("0x7B90337f65fAA2B2B8ed583ba1Ba6EB0C9D7eA44").unwrap(),
                EthBytes(vec![0xa]),
                BlockNumberOrHash::from_predefined(Predefined::Earliest),
            ))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::basic(
            EthGetStorageAt::request((
                EthAddress::from_str("0x7B90337f65fAA2B2B8ed583ba1Ba6EB0C9D7eA44").unwrap(),
                EthBytes(vec![0xa]),
                BlockNumberOrHash::from_predefined(Predefined::Pending),
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetStorageAt::request((
                EthAddress::from_str("0x7B90337f65fAA2B2B8ed583ba1Ba6EB0C9D7eA44").unwrap(),
                EthBytes(vec![0xa]),
                BlockNumberOrHash::from_predefined(Predefined::Latest),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetStorageAt::request((
                generate_eth_random_address().unwrap(),
                EthBytes(vec![0x0]),
                BlockNumberOrHash::from_predefined(Predefined::Latest),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetStorageAtV2::request((
                EthAddress::from_str("0x7B90337f65fAA2B2B8ed583ba1Ba6EB0C9D7eA44").unwrap(),
                EthBytes(vec![0xa]),
                ExtBlockNumberOrHash::from_block_number(shared_tipset.epoch()),
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetStorageAtV2::request((
                EthAddress::from_str("0x7B90337f65fAA2B2B8ed583ba1Ba6EB0C9D7eA44").unwrap(),
                EthBytes(vec![0xa]),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Safe),
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetStorageAtV2::request((
                EthAddress::from_str("0x7B90337f65fAA2B2B8ed583ba1Ba6EB0C9D7eA44").unwrap(),
                EthBytes(vec![0xa]),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Finalized),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetStorageAtV2::request((
                generate_eth_random_address().unwrap(),
                EthBytes(vec![0x0]),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Latest),
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
            EthFeeHistory::request((
                10.into(),
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Earliest),
                None,
            ))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::basic(
            EthFeeHistory::request((
                10.into(),
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Pending),
                Some(vec![10., 50., 90.]),
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthFeeHistory::request((
                10.into(),
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Latest),
                None,
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthFeeHistory::request((
                10.into(),
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Safe),
                None,
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthFeeHistory::request((
                10.into(),
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Finalized),
                Some(vec![10., 50., 90.]),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthFeeHistoryV2::request((
                10.into(),
                ExtBlockNumberOrHash::from_block_number(shared_tipset.epoch()),
                None,
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthFeeHistoryV2::request((
                10.into(),
                ExtBlockNumberOrHash::from_block_number(shared_tipset.epoch()),
                Some(vec![10., 50., 90.]),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthFeeHistoryV2::request((
                10.into(),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Earliest),
                None,
            ))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::basic(
            EthFeeHistoryV2::request((
                10.into(),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Pending),
                Some(vec![10., 50., 90.]),
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthFeeHistoryV2::request((
                10.into(),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Latest),
                None,
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthFeeHistoryV2::request((
                10.into(),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Safe),
                None,
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthFeeHistoryV2::request((
                10.into(),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Finalized),
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
            EthGetCode::request((
                EthAddress::from_str("0x7B90337f65fAA2B2B8ed583ba1Ba6EB0C9D7eA44").unwrap(),
                BlockNumberOrHash::from_predefined(Predefined::Earliest),
            ))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::basic(
            EthGetCode::request((
                EthAddress::from_str("0x7B90337f65fAA2B2B8ed583ba1Ba6EB0C9D7eA44").unwrap(),
                BlockNumberOrHash::from_predefined(Predefined::Pending),
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetCode::request((
                EthAddress::from_str("0x7B90337f65fAA2B2B8ed583ba1Ba6EB0C9D7eA44").unwrap(),
                BlockNumberOrHash::from_predefined(Predefined::Latest),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetCode::request((
                generate_eth_random_address().unwrap(),
                BlockNumberOrHash::from_predefined(Predefined::Latest),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetCodeV2::request((
                // https://filfox.info/en/address/f410fpoidg73f7krlfohnla52dotowde5p2sejxnd4mq
                EthAddress::from_str("0x7B90337f65fAA2B2B8ed583ba1Ba6EB0C9D7eA44").unwrap(),
                ExtBlockNumberOrHash::from_block_number(shared_tipset.epoch()),
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetCodeV2::request((
                EthAddress::from_str("0x7B90337f65fAA2B2B8ed583ba1Ba6EB0C9D7eA44").unwrap(),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Safe),
            ))
            .unwrap(),
        ),
        RpcTest::basic(
            EthGetCodeV2::request((
                EthAddress::from_str("0x7B90337f65fAA2B2B8ed583ba1Ba6EB0C9D7eA44").unwrap(),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Finalized),
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthGetCodeV2::request((
                generate_eth_random_address().unwrap(),
                ExtBlockNumberOrHash::from_predefined(ExtPredefined::Latest),
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
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::identity(
            EthGetTransactionByBlockNumberAndIndex::request((
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Earliest),
                0.into(),
            ))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::identity(
            EthGetTransactionByBlockNumberAndIndex::request((
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Pending),
                0.into(),
            ))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::identity(
            EthGetTransactionByBlockNumberAndIndex::request((
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Latest),
                0.into(),
            ))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::identity(
            EthGetTransactionByBlockNumberAndIndexV2::request((
                BlockNumberOrPredefined::BlockNumber(shared_tipset.epoch().into()),
                0.into(),
            ))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::identity(
            EthGetTransactionByBlockNumberAndIndexV2::request((
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Safe),
                0.into(),
            ))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::identity(
            EthGetTransactionByBlockNumberAndIndexV2::request((
                BlockNumberOrPredefined::PredefinedBlock(ExtPredefined::Finalized),
                0.into(),
            ))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::identity(
            EthGetTransactionByBlockHashAndIndex::request((block_hash, 0.into())).unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithIdenticalError),
        RpcTest::identity(EthGetBlockByHash::request((block_hash, false)).unwrap()),
        RpcTest::identity(EthGetBlockByHash::request((block_hash, true)).unwrap()),
        RpcTest::identity(
            EthGetLogs::request((EthFilterSpec {
                from_block: Some(format!("0x{:x}", shared_tipset.epoch())),
                to_block: Some(format!("0x{:x}", shared_tipset.epoch())),
                ..Default::default()
            },))
            .unwrap(),
        )
        .sort_policy(SortPolicy::All)
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::identity(
            EthGetLogs::request((EthFilterSpec {
                from_block: Some(format!("0x{:x}", shared_tipset.epoch())),
                to_block: Some(format!("0x{:x}", shared_tipset.epoch())),
                address: Some(EthAddressList::List(Vec::new())),
                ..Default::default()
            },))
            .unwrap(),
        )
        .sort_policy(SortPolicy::All)
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::identity(
            EthGetLogs::request((EthFilterSpec {
                from_block: Some(format!("0x{:x}", shared_tipset.epoch() - 100)),
                to_block: Some(format!("0x{:x}", shared_tipset.epoch())),
                ..Default::default()
            },))
            .unwrap(),
        )
        .sort_policy(SortPolicy::All)
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::identity(
            EthGetLogs::request((EthFilterSpec {
                address: Some(EthAddressList::Single(
                    EthAddress::from_str("0x7B90337f65fAA2B2B8ed583ba1Ba6EB0C9D7eA44").unwrap(),
                )),
                ..Default::default()
            },))
            .unwrap(),
        )
        .sort_policy(SortPolicy::All)
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::identity(EthGetFilterLogs::request((FilterID::new().unwrap(),)).unwrap())
            .policy_on_rejected(PolicyOnRejected::PassWithIdenticalError),
        RpcTest::identity(EthGetFilterChanges::request((FilterID::new().unwrap(),)).unwrap())
            .policy_on_rejected(PolicyOnRejected::PassWithIdenticalError),
        RpcTest::identity(EthGetTransactionHashByCid::request((block_cid,)).unwrap()),
        RpcTest::identity(
            EthTraceBlock::request((ExtBlockNumberOrHash::from_block_number(
                shared_tipset.epoch(),
            ),))
            .unwrap(),
        ),
        RpcTest::identity(
            EthTraceBlock::request((ExtBlockNumberOrHash::from_predefined(
                ExtPredefined::Earliest,
            ),))
            .unwrap(),
        )
        .policy_on_rejected(PolicyOnRejected::PassWithQuasiIdenticalError),
        RpcTest::basic(
            EthTraceBlock::request((ExtBlockNumberOrHash::from_predefined(
                ExtPredefined::Pending,
            ),))
            .unwrap(),
        ),
        RpcTest::basic(
            EthTraceBlock::request((ExtBlockNumberOrHash::from_predefined(ExtPredefined::Latest),))
                .unwrap(),
        ),
        RpcTest::basic(
            EthTraceBlock::request((ExtBlockNumberOrHash::from_predefined(ExtPredefined::Safe),))
                .unwrap(),
        ),
        RpcTest::basic(
            EthTraceBlock::request((ExtBlockNumberOrHash::from_predefined(
                ExtPredefined::Finalized,
            ),))
            .unwrap(),
        ),
        RpcTest::identity(
            EthTraceBlockV2::request((ExtBlockNumberOrHash::from_block_number(
                shared_tipset.epoch(),
            ),))
            .unwrap(),
        ),
        RpcTest::basic(
            EthTraceBlockV2::request((ExtBlockNumberOrHash::from_predefined(
                ExtPredefined::Pending,
            ),))
            .unwrap(),
        ),
        RpcTest::basic(
            EthTraceBlockV2::request((ExtBlockNumberOrHash::from_predefined(
                ExtPredefined::Latest,
            ),))
            .unwrap(),
        ),
        RpcTest::basic(
            EthTraceBlockV2::request((ExtBlockNumberOrHash::from_predefined(ExtPredefined::Safe),))
                .unwrap(),
        ),
        RpcTest::basic(
            EthTraceBlockV2::request((ExtBlockNumberOrHash::from_predefined(
                ExtPredefined::Finalized,
            ),))
            .unwrap(),
        ),
        RpcTest::identity(
            EthTraceReplayBlockTransactions::request((
                ExtBlockNumberOrHash::from_block_number(shared_tipset.epoch()),
                vec!["trace".to_string()],
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthTraceReplayBlockTransactionsV2::request((
                ExtBlockNumberOrHash::from_block_number(shared_tipset.epoch()),
                vec!["trace".to_string()],
            ))
            .unwrap(),
        ),
        RpcTest::identity(
            EthTraceFilter::request((EthTraceFilterCriteria {
                from_block: Some(format!("0x{:x}", shared_tipset.epoch() - 100)),
                to_block: Some(format!("0x{:x}", shared_tipset.epoch() - SAFE_EPOCH_DELAY)),
                ..Default::default()
            },))
            .unwrap(),
        )
        // both nodes could fail on, e.g., "too many results, maximum supported is 500, try paginating
        // requests with After and Count"
        .policy_on_rejected(PolicyOnRejected::PassWithIdenticalError),
        RpcTest::identity(
            EthGetTransactionReceipt::request((
                // A transaction that should not exist, to test the `null` response in case
                // of missing transaction.
                EthHash::from_str(
                    "0xf234567890123456789d6a7b8c9d0e1f2a3b4c5d6e7f8091a2b3c4d5e6f70809",
                )
                .unwrap(),
            ))
            .unwrap(),
        ),
    ];

    for block in shared_tipset.block_headers() {
        tests.extend([RpcTest::identity(
            FilecoinAddressToEthAddress::request((
                block.miner_address,
                Some(BlockNumberOrPredefined::PredefinedBlock(
                    ExtPredefined::Latest,
                )),
            ))
            .unwrap(),
        )]);
        let (bls_messages, secp_messages) =
            crate::chain::store::block_messages(store, block).unwrap();
        for msg in sample_messages(bls_messages.iter(), secp_messages.iter()) {
            tests.extend([RpcTest::identity(
                FilecoinAddressToEthAddress::request((
                    msg.from(),
                    Some(BlockNumberOrPredefined::PredefinedBlock(
                        ExtPredefined::Latest,
                    )),
                ))
                .unwrap(),
            )]);
            if let Ok(eth_to_addr) = EthAddress::try_from(msg.to) {
                tests.extend([RpcTest::identity(
                    EthEstimateGas::request((
                        EthCallMessage {
                            to: Some(eth_to_addr),
                            value: Some(msg.value.clone().into()),
                            data: Some(msg.params.clone().into()),
                            ..Default::default()
                        },
                        Some(BlockNumberOrHash::BlockNumber(shared_tipset.epoch().into())),
                    ))
                    .unwrap(),
                )
                .policy_on_rejected(PolicyOnRejected::Pass)]);
                tests.extend([RpcTest::identity(
                    EthEstimateGasV2::request((
                        EthCallMessage {
                            to: Some(eth_to_addr),
                            value: Some(msg.value.clone().into()),
                            data: Some(msg.params.clone().into()),
                            ..Default::default()
                        },
                        Some(ExtBlockNumberOrHash::BlockNumber(
                            shared_tipset.epoch().into(),
                        )),
                    ))
                    .unwrap(),
                )
                .policy_on_rejected(PolicyOnRejected::Pass)]);
            }
        }
    }

    tests
}

fn read_state_api_tests(tipset: &Tipset) -> anyhow::Result<Vec<RpcTest>> {
    let tests = vec![
        RpcTest::identity(StateReadState::request((
            Address::SYSTEM_ACTOR,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateReadState::request((
            Address::SYSTEM_ACTOR,
            Default::default(),
        ))?),
        RpcTest::identity(StateReadState::request((
            Address::CRON_ACTOR,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateReadState::request((
            Address::MARKET_ACTOR,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateReadState::request((
            Address::INIT_ACTOR,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateReadState::request((
            Address::POWER_ACTOR,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateReadState::request((
            Address::REWARD_ACTOR,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateReadState::request((
            Address::VERIFIED_REGISTRY_ACTOR,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateReadState::request((
            Address::DATACAP_TOKEN_ACTOR,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateReadState::request((
            // payment channel actor address `t066116`
            Address::new_id(66116), // https://calibration.filscan.io/en/address/t066116/
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateReadState::request((
            // multisig actor address `t018101`
            Address::new_id(18101), // https://calibration.filscan.io/en/address/t018101/
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateReadState::request((
            ACCOUNT_ADDRESS,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateReadState::request((
            MINER_ADDRESS,
            tipset.key().into(),
        ))?),
        RpcTest::identity(StateReadState::request((
            Address::from_str(EVM_ADDRESS).unwrap(), // evm actor
            tipset.key().into(),
        ))?),
    ];

    Ok(tests)
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
            tests.push(RpcTest::identity(EthGetTransactionByHash::request((
                tx.hash,
            ))?));
            tests.push(RpcTest::identity(EthGetTransactionByHashLimited::request(
                (tx.hash, shared_tipset.epoch()),
            )?));
            tests.push(RpcTest::identity(EthTraceTransaction::request((tx
                .hash
                .to_string(),))?));
            if smsg.message.from.protocol() == Protocol::Delegated
                && smsg.message.to.protocol() == Protocol::Delegated
            {
                tests.push(
                    RpcTest::identity(EthGetTransactionReceipt::request((tx.hash,))?)
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

    // Test eth_call API errors
    tests.extend(eth_call_api_err_tests(shared_tipset.epoch()));

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

    vec![
        // The tipset is only used for resolving the 'from' address and not when
        // computing the gas cost. This means that the `GasEstimateGasLimit` method
        // is inherently non-deterministic, but I'm fairly sure we're compensated for
        // everything. If not, this test will be flaky. Instead of disabling it, we
        // should relax the verification requirement.
        RpcTest::identity(
            GasEstimateGasLimit::request((message.clone(), shared_tipset.key().into())).unwrap(),
        ),
        // Gas estimation is inherently non-deterministic due to randomness in gas premium
        // calculation and network state changes. We validate that both implementations
        // return reasonable values within expected bounds rather than exact equality.
        RpcTest::validate(
            GasEstimateMessageGas::request((
                message,
                None, // No MessageSendSpec
                shared_tipset.key().into(),
            ))
            .unwrap(),
            |forest_api_msg, lotus_api_msg| {
                let forest_msg = forest_api_msg.message;
                let lotus_msg = lotus_api_msg.message;
                // Validate that the gas limit is identical (must be deterministic)
                if forest_msg.gas_limit != lotus_msg.gas_limit {
                    return false;
                }

                // Validate gas fee cap and premium are within reasonable bounds (±5%)
                let forest_fee_cap = &forest_msg.gas_fee_cap;
                let lotus_fee_cap = &lotus_msg.gas_fee_cap;
                let forest_premium = &forest_msg.gas_premium;
                let lotus_premium = &lotus_msg.gas_premium;

                // Gas fee cap and premium should not be negative
                if [forest_fee_cap, lotus_fee_cap, forest_premium, lotus_premium]
                    .iter()
                    .any(|amt| amt.is_negative())
                {
                    return false;
                }

                forest_fee_cap.is_within_percent(lotus_fee_cap, 5)
                    && forest_premium.is_within_percent(lotus_premium, 5)
            },
        ),
    ]
}

fn f3_tests() -> anyhow::Result<Vec<RpcTest>> {
    Ok(vec![
        // using basic because 2 nodes are not guaranteed to be at the same head
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
        RpcTest::identity(F3GetCertificate::request((50,))?),
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
    offline: bool,
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
        tests.extend(chain_tests_with_tipset(&store, offline, &tipset)?);
        tests.extend(miner_tests_with_tipset(&store, &tipset, miner_address)?);
        tests.extend(state_tests_with_tipset(&store, &tipset)?);
        tests.extend(eth_tests_with_tipset(&store, &tipset));
        tests.extend(event_tests_with_tipset(&store, &tipset));
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

pub(super) async fn create_tests(
    CreateTestsArgs {
        offline,
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
    tests.extend(f3_tests()?);
    if !snapshot_files.is_empty() {
        let store = Arc::new(ManyCar::try_from(snapshot_files.clone())?);
        revalidate_chain(store.clone(), n_tipsets).await?;
        tests.extend(snapshot_tests(
            store,
            offline,
            n_tipsets,
            miner_address,
            eth_chain_id,
        )?);
    }
    tests.sort_by_key(|test| test.request.method_name.clone());

    tests.extend(create_deferred_tests(snapshot_files)?);
    Ok(tests)
}

// Some tests, especially those mutating the node's state, need to be run last.
fn create_deferred_tests(snapshot_files: Vec<PathBuf>) -> anyhow::Result<Vec<RpcTest>> {
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

async fn revalidate_chain(db: Arc<ManyCar>, n_ts_to_validate: usize) -> anyhow::Result<()> {
    if n_ts_to_validate == 0 {
        return Ok(());
    }
    let chain_config = Arc::new(handle_chain_config(&NetworkChain::Calibnet)?);

    let genesis_header = crate::genesis::read_genesis_header(
        None,
        chain_config.genesis_bytes(&db).await?.as_deref(),
        &db,
    )
    .await?;
    let chain_store = Arc::new(ChainStore::new(
        db.clone(),
        db.clone(),
        db.clone(),
        chain_config,
        genesis_header.clone(),
    )?);
    let state_manager = Arc::new(StateManager::new(chain_store.clone())?);
    let head_ts = db.heaviest_tipset()?;

    // Set proof parameter data dir and make sure the proofs are available. Otherwise,
    // validation might fail due to missing proof parameters.
    proofs_api::maybe_set_proofs_parameter_cache_dir_env(&Config::default().client.data_dir);
    ensure_proof_params_downloaded().await?;
    state_manager.validate_tipsets(
        head_ts
            .chain(&db)
            .take(SAFE_EPOCH_DELAY as usize + n_ts_to_validate),
    )?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn run_tests(
    tests: impl IntoIterator<Item = RpcTest>,
    forest: impl Into<Arc<rpc::Client>>,
    lotus: impl Into<Arc<rpc::Client>>,
    max_concurrent_requests: usize,
    filter_file: Option<PathBuf>,
    filter: String,
    filter_version: Option<rpc::ApiPaths>,
    run_ignored: RunIgnored,
    fail_fast: bool,
    dump_dir: Option<PathBuf>,
    test_criteria_overrides: &[TestCriteriaOverride],
    report_dir: Option<PathBuf>,
    report_mode: ReportMode,
    n_retries: usize,
) -> anyhow::Result<()> {
    let forest = Into::<Arc<rpc::Client>>::into(forest);
    let lotus = Into::<Arc<rpc::Client>>::into(lotus);
    let semaphore = Arc::new(Semaphore::new(max_concurrent_requests));
    let mut tasks = JoinSet::new();

    let filter_list = if let Some(filter_file) = &filter_file {
        FilterList::new_from_file(filter_file)?
    } else {
        FilterList::default().allow(filter.clone())
    };

    // Always use ReportBuilder for consistency
    let mut report_builder = ReportBuilder::new(&filter_list, report_mode);

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

        if let Some(filter_version) = filter_version
            && !test.request.api_paths.contains(filter_version)
        {
            continue;
        }

        // Acquire a permit from the semaphore before spawning a test
        let semaphore = semaphore.clone();
        let forest = forest.clone();
        let lotus = lotus.clone();
        let test_criteria_overrides = test_criteria_overrides.to_vec();
        tasks.spawn(async move {
            let mut n_retries_left = n_retries;
            let mut backoff_secs = 2;
            loop {
                {
                    // Ignore the error since 'An acquire operation can only fail if the semaphore has been closed'
                    let _permit = semaphore.acquire().await;
                    let test_result = test.run(&forest, &lotus).await;
                    let success =
                        evaluate_test_success(&test_result, &test, &test_criteria_overrides);
                    if success || n_retries_left == 0 {
                        return (success, test, test_result);
                    }
                    // Release the semaphore before sleeping
                }
                // Sleep before each retry
                tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                n_retries_left = n_retries_left.saturating_sub(1);
                backoff_secs = backoff_secs.saturating_mul(2);
            }
        });
    }

    // If no tests to run after filtering, return early without saving/printing
    if tasks.is_empty() {
        return Ok(());
    }

    while let Some(result) = tasks.join_next().await {
        match result {
            Ok((success, test, test_result)) => {
                let method_name = test.request.method_name.clone();

                report_builder.track_test_result(
                    method_name.as_ref(),
                    success,
                    &test_result,
                    &test.request.params,
                );

                // Dump test data if configured
                if let (Some(dump_dir), Some(test_dump)) = (&dump_dir, &test_result.test_dump) {
                    dump_test_data(dump_dir, success, test_dump)?;
                }

                if !success && fail_fast {
                    break;
                }
            }
            Err(e) => tracing::warn!("{e}"),
        }
    }

    let has_failures = report_builder.has_failures();
    report_builder.print_summary();

    if let Some(path) = report_dir {
        report_builder.finalize_and_save(&path)?;
    }

    anyhow::ensure!(!has_failures, "Some tests failed");

    Ok(())
}

/// Evaluate whether a test is successful based on the test result and criteria
fn evaluate_test_success(
    test_result: &TestResult,
    test: &RpcTest,
    test_criteria_overrides: &[TestCriteriaOverride],
) -> bool {
    match (&test_result.forest_status, &test_result.lotus_status) {
        (TestSummary::Valid, TestSummary::Valid) => true,
        (TestSummary::Valid, TestSummary::Timeout) => {
            test_criteria_overrides.contains(&TestCriteriaOverride::ValidAndTimeout)
        }
        (TestSummary::Timeout, TestSummary::Timeout) => {
            test_criteria_overrides.contains(&TestCriteriaOverride::TimeoutAndTimeout)
        }
        (TestSummary::Rejected(reason_forest), TestSummary::Rejected(reason_lotus)) => {
            match test.policy_on_rejected {
                PolicyOnRejected::Pass => true,
                PolicyOnRejected::PassWithIdenticalError => reason_forest == reason_lotus,
                PolicyOnRejected::PassWithIdenticalErrorCaseInsensitive => {
                    reason_forest.eq_ignore_ascii_case(reason_lotus)
                }
                PolicyOnRejected::PassWithQuasiIdenticalError => {
                    reason_lotus.contains(reason_forest) || reason_forest.contains(reason_lotus)
                }
                _ => false,
            }
        }
        _ => false,
    }
}

fn normalized_error_message(s: &str) -> Cow<'_, str> {
    // remove `RPC error (-32603):` prefix added by `lotus-gateway`
    let lotus_gateway_error_prefix = lazy_regex::regex!(r#"^RPC\serror\s\(-?\d+\):\s*"#);
    lotus_gateway_error_prefix.replace(s, "")
}

/// Dump test data to the specified directory
fn dump_test_data(dump_dir: &Path, success: bool, test_dump: &TestDump) -> anyhow::Result<()> {
    let dir = dump_dir.join(if success { "valid" } else { "invalid" });
    if !dir.is_dir() {
        std::fs::create_dir_all(&dir)?;
    }
    let file_name = format!(
        "{}_{}.json",
        test_dump
            .request
            .method_name
            .as_ref()
            .replace(".", "_")
            .to_lowercase(),
        Utc::now().timestamp_micros()
    );
    std::fs::write(
        dir.join(file_name),
        serde_json::to_string_pretty(test_dump)?,
    )?;
    Ok(())
}

fn validate_message_lookup(req: rpc::Request<MessageLookup>) -> RpcTest {
    RpcTest::validate(req, |mut forest, mut lotus| {
        // TODO(hanabi1224): https://github.com/ChainSafe/forest/issues/3784
        forest.return_dec = Ipld::Null;
        lotus.return_dec = Ipld::Null;
        forest == lotus
    })
}

fn validate_tagged_tipset_v2(req: rpc::Request<Option<Tipset>>, offline: bool) -> RpcTest {
    RpcTest::validate(req, move |forest, lotus| match (forest, lotus) {
        (None, None) => true,
        (Some(forest), Some(lotus)) => {
            if offline {
                true
            } else {
                (forest.epoch() - lotus.epoch()).abs() <= 2
            }
        }
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalized_error_message_1() {
        let s = "RPC error (-32603): exactly one tipset selection criteria must be specified";
        let r = normalized_error_message(s);
        assert_eq!(
            r.as_ref(),
            "exactly one tipset selection criteria must be specified"
        );
    }

    #[test]
    fn test_normalized_error_message_2() {
        let s = "exactly one tipset selection criteria must be specified";
        let r = normalized_error_message(s);
        assert_eq!(
            r.as_ref(),
            "exactly one tipset selection criteria must be specified"
        );
    }
}
