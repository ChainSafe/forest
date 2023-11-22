// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashMap;
use clap::Subcommand;
use fil_actors_shared::v10::runtime::DomainSeparationTag;
use serde::de::DeserializeOwned;
use std::path::PathBuf;
use std::str::FromStr;
use tabled::{builder::Builder, settings::Style};

use crate::blocks::Tipset;
use crate::blocks::TipsetKeys;
use crate::cid_collections::CidHashSet;
use crate::db::car::ManyCar;
use crate::lotus_json::HasLotusJson;
use crate::message::Message as _;
use crate::rpc_client::{ApiInfo, JsonRpcError, RpcRequest};
use crate::shim::address::{Address, Protocol};
use crate::shim::crypto::Signature;

#[derive(Debug, Subcommand)]
pub enum ApiCommands {
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
        /// Cancel test run on the first failure
        #[arg(long)]
        fail_fast: bool,
    },
}

impl ApiCommands {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Compare {
                forest,
                lotus,
                snapshot_files,
                filter,
                fail_fast,
            } => compare_apis(forest, lotus, snapshot_files, filter, fail_fast).await?,
        }
        Ok(())
    }
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
    Valid,
}

impl EndpointStatus {
    fn from_json_error(err: JsonRpcError) -> Self {
        if err.code == JsonRpcError::INVALID_REQUEST.code {
            EndpointStatus::InvalidRequest
        } else if err.code == JsonRpcError::METHOD_NOT_FOUND.code {
            EndpointStatus::MissingMethod
        } else if err.code == JsonRpcError::PARSE_ERROR.code {
            EndpointStatus::InvalidResponse
        } else {
            EndpointStatus::InternalServerError
        }
    }
}

struct RpcTest {
    request: RpcRequest,
    check_syntax: Box<dyn Fn(serde_json::Value) -> bool>,
    check_semantics: Box<dyn Fn(serde_json::Value, serde_json::Value) -> bool>,
}

impl RpcTest {
    // Check that an endpoint exist and that both the Lotus and Forest JSON
    // response follows the same schema.
    fn basic<T: DeserializeOwned>(request: RpcRequest<T>) -> RpcTest
    where
        T: HasLotusJson,
    {
        RpcTest {
            request: request.lower(),
            check_syntax: Box::new(|value| serde_json::from_value::<T::LotusJson>(value).is_ok()),
            check_semantics: Box::new(|_, _| true),
        }
    }

    // Check that an endpoint exist, has the same JSON schema, and do custom
    // validation over both responses.
    fn validate<T>(request: RpcRequest<T>, validate: impl Fn(T, T) -> bool + 'static) -> RpcTest
    where
        T: HasLotusJson,
        T::LotusJson: DeserializeOwned,
    {
        RpcTest {
            request: request.lower(),
            check_syntax: Box::new(|value| serde_json::from_value::<T::LotusJson>(value).is_ok()),
            check_semantics: Box::new(move |forest_json, lotus_json| {
                serde_json::from_value::<T::LotusJson>(forest_json).is_ok_and(|forest| {
                    serde_json::from_value::<T::LotusJson>(lotus_json).is_ok_and(|lotus| {
                        validate(
                            HasLotusJson::from_lotus_json(forest),
                            HasLotusJson::from_lotus_json(lotus),
                        )
                    })
                })
            }),
        }
    }

    // Check that an endpoint exist and that Forest returns exactly the same
    // JSON as Lotus.
    fn identity<T: PartialEq>(request: RpcRequest<T>) -> RpcTest
    where
        T: HasLotusJson,
        T::LotusJson: DeserializeOwned,
    {
        RpcTest::validate(request, |forest, lotus| forest == lotus)
    }

    async fn run(
        &self,
        forest_api: &ApiInfo,
        lotus_api: &ApiInfo,
    ) -> (EndpointStatus, EndpointStatus) {
        let forest_resp = forest_api.call(self.request.clone()).await;
        let lotus_resp = lotus_api.call(self.request.clone()).await;

        match (forest_resp, lotus_resp) {
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
        }
    }
}

fn common_tests() -> Vec<RpcTest> {
    vec![
        RpcTest::basic(ApiInfo::version_req()),
        RpcTest::basic(ApiInfo::start_time_req()),
        RpcTest::basic(ApiInfo::discover_req()),
        RpcTest::basic(ApiInfo::session_req()),
    ]
}

fn auth_tests() -> Vec<RpcTest> {
    // Auth commands should be tested as well. Tracking issue:
    // https://github.com/ChainSafe/forest/issues/3639
    vec![]
}

fn chain_tests() -> Vec<RpcTest> {
    vec![
        RpcTest::validate(ApiInfo::chain_head_req(), |forest, lotus| {
            forest.epoch().abs_diff(lotus.epoch()) < 10
        }),
        RpcTest::identity(ApiInfo::chain_get_genesis_req()),
    ]
}

fn chain_tests_with_tipset(shared_tipset: &Tipset) -> Vec<RpcTest> {
    let shared_block = shared_tipset.min_ticket_block();

    vec![
        RpcTest::identity(ApiInfo::chain_get_block_req(*shared_block.cid())),
        RpcTest::identity(ApiInfo::chain_get_tipset_by_height_req(
            shared_tipset.epoch(),
            TipsetKeys::default(),
        )),
        RpcTest::identity(ApiInfo::chain_get_tipset_req(shared_tipset.key().clone())),
        RpcTest::identity(ApiInfo::chain_read_obj_req(*shared_block.cid())),
    ]
}

fn mpool_tests() -> Vec<RpcTest> {
    vec![RpcTest::basic(ApiInfo::mpool_pending_req(vec![]))]
}

fn net_tests() -> Vec<RpcTest> {
    // More net commands should be tested. Tracking issue:
    // https://github.com/ChainSafe/forest/issues/3639
    vec![
        RpcTest::basic(ApiInfo::net_addrs_listen_req()),
        RpcTest::basic(ApiInfo::net_peers_req()),
        RpcTest::basic(ApiInfo::net_info_req()),
    ]
}

fn node_tests() -> Vec<RpcTest> {
    vec![
        // This is a v1 RPC call. We don't support any v1 calls yet. Tracking
        // issue: https://github.com/ChainSafe/forest/issues/3640
        //RpcTest::basic(ApiInfo::node_status_req())
    ]
}

fn state_tests(shared_tipset: &Tipset) -> Vec<RpcTest> {
    let shared_block = shared_tipset.min_ticket_block();
    vec![
        RpcTest::identity(ApiInfo::state_network_name_req()),
        RpcTest::identity(ApiInfo::state_get_actor_req(
            Address::SYSTEM_ACTOR,
            shared_tipset.key().clone(),
        )),
        RpcTest::identity(ApiInfo::state_get_randomness_from_beacon_req(
            shared_tipset.key().clone(),
            DomainSeparationTag::ElectionProofProduction,
            shared_tipset.epoch(),
            "dead beef".as_bytes().to_vec(),
        )),
        RpcTest::identity(ApiInfo::state_read_state_req(
            Address::SYSTEM_ACTOR,
            shared_tipset.key().clone(),
        )),
        RpcTest::identity(ApiInfo::state_read_state_req(
            Address::SYSTEM_ACTOR,
            TipsetKeys::from_iter(Vec::new()),
        )),
        RpcTest::identity(ApiInfo::state_miner_active_sectors_req(
            *shared_block.miner_address(),
            shared_tipset.key().clone(),
        )),
        RpcTest::identity(ApiInfo::state_lookup_id_req(
            *shared_block.miner_address(),
            shared_tipset.key().clone(),
        )),
        // This should return `Address::new_id(0xdeadbeef)`
        RpcTest::identity(ApiInfo::state_lookup_id_req(
            Address::new_id(0xdeadbeef),
            shared_tipset.key().clone(),
        )),
        RpcTest::identity(ApiInfo::state_network_version_req(
            shared_tipset.key().clone(),
        )),
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
        RpcTest::identity(ApiInfo::wallet_balance_req(known_wallet.to_string())),
        RpcTest::identity(ApiInfo::wallet_verify_req(known_wallet, text, signature)),
        // These methods require write access in Lotus. Not sure why.
        // RpcTest::basic(ApiInfo::wallet_default_address_req()),
        // RpcTest::basic(ApiInfo::wallet_list_req()),
        // RpcTest::basic(ApiInfo::wallet_has_req(known_wallet.to_string())),
    ]
}

// Extract tests that use chain-specific data such as block CIDs or message
// CIDs. Right now, only the last 20 tipsets are used. It would be nice to
// sample a greater range.
fn snapshot_tests(store: &ManyCar) -> anyhow::Result<Vec<RpcTest>> {
    let mut tests = vec![];
    let shared_tipset = store.heaviest_tipset()?;
    let root_tsk = shared_tipset.key().clone();
    tests.extend(chain_tests_with_tipset(&shared_tipset));
    tests.extend(state_tests(&shared_tipset));

    let mut seen = CidHashSet::default();
    for tipset in shared_tipset.chain(&store).take(20) {
        tests.push(RpcTest::identity(
            ApiInfo::chain_get_messages_in_tipset_req(tipset.key().clone()),
        ));
        for block in tipset.blocks() {
            tests.push(RpcTest::identity(ApiInfo::chain_get_block_messages_req(
                *block.cid(),
            )));
            tests.push(RpcTest::identity(ApiInfo::chain_get_parent_messages_req(
                *block.cid(),
            )));

            let (bls_messages, secp_messages) = crate::chain::store::block_messages(&store, block)?;
            for msg in bls_messages {
                if seen.insert(msg.cid()?) {
                    tests.push(RpcTest::identity(ApiInfo::chain_get_message_req(
                        msg.cid()?,
                    )));
                    tests.push(RpcTest::identity(ApiInfo::state_account_key_req(
                        msg.from(),
                        root_tsk.clone(),
                    )));
                }
            }
            for msg in secp_messages {
                if seen.insert(msg.cid()?) {
                    tests.push(RpcTest::identity(ApiInfo::chain_get_message_req(
                        msg.cid()?,
                    )));
                    tests.push(RpcTest::identity(ApiInfo::state_account_key_req(
                        msg.from(),
                        root_tsk.clone(),
                    )));
                    if !msg.params().is_empty() {
                        tests.push(RpcTest::identity(ApiInfo::state_decode_params_req(
                            msg.to(),
                            msg.method_num(),
                            msg.params().to_vec(),
                            root_tsk.clone(),
                        )));
                    }
                }
            }
            tests.push(RpcTest::identity(ApiInfo::state_miner_power_req(
                *block.miner_address(),
                tipset.key().clone(),
            )));
            tests.push(RpcTest::identity(ApiInfo::state_miner_faults_req(
                *block.miner_address(),
                tipset.key().clone(),
            )))
        }
        tests.push(RpcTest::basic(ApiInfo::state_circulating_supply_req(
            tipset.key().clone(),
        )))
    }
    Ok(tests)
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
async fn compare_apis(
    forest: ApiInfo,
    lotus: ApiInfo,
    snapshot_files: Vec<PathBuf>,
    filter: String,
    fail_fast: bool,
) -> anyhow::Result<()> {
    let mut tests = vec![];

    tests.extend(common_tests());
    tests.extend(auth_tests());
    tests.extend(chain_tests());
    tests.extend(mpool_tests());
    tests.extend(net_tests());
    tests.extend(node_tests());
    tests.extend(wallet_tests());

    if !snapshot_files.is_empty() {
        let store = ManyCar::try_from(snapshot_files)?;
        tests.extend(snapshot_tests(&store)?);
    }

    tests.sort_by_key(|test| test.request.method_name);

    let mut results = HashMap::default();

    for test in tests.into_iter() {
        if !test.request.method_name.contains(&filter) {
            continue;
        }
        let (forest_status, lotus_status) = test.run(&forest, &lotus).await;
        results
            .entry((test.request.method_name, forest_status, lotus_status))
            .and_modify(|v| *v += 1)
            .or_insert(1u32);
        if (forest_status != EndpointStatus::Valid || lotus_status != EndpointStatus::Valid)
            && fail_fast
        {
            break;
        }
    }

    let mut results = results.into_iter().collect::<Vec<_>>();
    results.sort();
    println!("{}", format_as_markdown(&results));

    Ok(())
}

fn format_as_markdown(results: &[((&'static str, EndpointStatus, EndpointStatus), u32)]) -> String {
    let mut builder = Builder::default();

    builder.set_header(["RPC Method", "Forest", "Lotus"]);

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
