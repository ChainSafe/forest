// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashMap;
// use ahash::HashSet;
// use chrono::Duration;
use clap::Subcommand;
// use libp2p::Multiaddr;
use serde::de::DeserializeOwned;
use std::str::FromStr;
use tabled::{builder::Builder, settings::Style};

use crate::blocks::Tipset;
use crate::blocks::TipsetKeys;
use crate::lotus_json::HasLotusJson;
// use crate::rpc_api::data_types::AddrInfo;
use crate::rpc_client::{ApiInfo, JsonRpcError, RpcRequest};
use crate::shim::address::Address;

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
    },
}

impl ApiCommands {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::Compare { forest, lotus } => compare_apis(forest, lotus).await?,
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialOrd, Ord, PartialEq, Eq, Hash)]
enum EndpointStatus {
    // RPC endpoint is missing (currently not reported correctly by either Forest nor Lotus)
    Missing,
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
        // dbg!(&err_message(&err));
        // dbg!(&err_code(&err));
        if err.code == JsonRpcError::INVALID_REQUEST.code {
            EndpointStatus::InvalidRequest
        } else if err.code == JsonRpcError::METHOD_NOT_FOUND.code {
            EndpointStatus::Missing
        } else if err.code == JsonRpcError::INVALID_REQUEST.code {
            EndpointStatus::InvalidJSON
        } else if err.code == JsonRpcError::PARSE_ERROR.code {
            EndpointStatus::InvalidResponse
        } else {
            EndpointStatus::InternalServerError
        }
    }
}

async fn youngest_tipset(forest: &ApiInfo, lotus: &ApiInfo) -> anyhow::Result<Tipset> {
    let t1 = forest.chain_head().await?;
    let t2 = lotus.chain_head().await?;
    if t1.epoch() < t2.epoch() {
        Ok(t1)
    } else {
        Ok(t2)
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

        // dbg!(self.request.method_name);
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
    ]
}

fn auth_tests() -> Vec<RpcTest> {
    // vec![RpcTest::basic(ApiInfo::auth_new_req(
    //     vec!["read".to_string()],
    //     Duration::days(1),
    // ))]
    vec![]
}

fn chain_tests(shared_tipset: &Tipset) -> Vec<RpcTest> {
    let shared_block = shared_tipset.min_ticket_block();

    vec![
        RpcTest::validate(ApiInfo::chain_head_req(), |forest, lotus| {
            forest.epoch().abs_diff(lotus.epoch()) < 10
        }),
        RpcTest::identity(ApiInfo::chain_get_block_req(*shared_block.cid())),
        RpcTest::identity(ApiInfo::chain_get_tipset_by_height_req(
            shared_tipset.epoch(),
            TipsetKeys::default(),
        )),
        RpcTest::identity(ApiInfo::chain_get_genesis_req()),
        RpcTest::identity(ApiInfo::chain_read_obj_req(*shared_block.cid())),
        // requires admin rights
        // RpcTest::identity(ApiInfo::chain_get_min_base_fee_req(20)),
    ]
}

fn mpool_tests() -> Vec<RpcTest> {
    vec![RpcTest::basic(ApiInfo::mpool_pending_req(vec![]))]
}

fn net_tests() -> Vec<RpcTest> {
    // let peer: Multiaddr = "/dns4/bootstrap-0.calibration.fildev.network/tcp/1347/p2p/12D3KooWCi2w8U4DDB9xqrejb5KYHaQv2iA2AJJ6uzG3iQxNLBMy".parse().unwrap();
    // let addr_info = AddrInfo {
    //     id: "12D3KooWCi2w8U4DDB9xqrejb5KYHaQv2iA2AJJ6uzG3iQxNLBMy".to_string(),
    //     addrs: HashSet::from_iter([peer]),
    // };
    vec![
        RpcTest::basic(ApiInfo::net_addrs_listen_req()),
        RpcTest::basic(ApiInfo::net_peers_req()),
        RpcTest::basic(ApiInfo::net_info_req()),
        // requires write access
        // RpcTest::basic(ApiInfo::net_connect_req(addr_info)),
    ]
}

fn node_tests() -> Vec<RpcTest> {
    vec![
        // This is a v1 RPC call. We don't support any v1 calls yet.
        //RpcTest::basic(ApiInfo::node_status_req())
    ]
}

fn state_tests(shared_tipset: &Tipset) -> Vec<RpcTest> {
    vec![
        RpcTest::identity(ApiInfo::state_network_name_req()),
        RpcTest::identity(ApiInfo::state_get_actor_req(
            Address::SYSTEM_ACTOR,
            shared_tipset.key().clone(),
        )),
    ]
}

async fn compare_apis(forest: ApiInfo, lotus: ApiInfo) -> anyhow::Result<()> {
    let shared_tipset = youngest_tipset(&forest, &lotus).await?;

    let mut tests = vec![];

    tests.extend(common_tests());
    tests.extend(auth_tests());
    tests.extend(chain_tests(&shared_tipset));
    tests.extend(mpool_tests());
    tests.extend(net_tests());
    tests.extend(node_tests());
    tests.extend(state_tests(&shared_tipset));

    let mut results = HashMap::default();

    for test in tests.into_iter() {
        let (forest_status, lotus_status) = test.run(&forest, &lotus).await;
        results
            .entry((test.request.method_name, forest_status, lotus_status))
            .and_modify(|v| *v += 1)
            .or_insert(1u32);
    }

    let mut results = results.into_iter().collect::<Vec<_>>();
    results.sort();
    output_markdown(&results);

    Ok(())
}

fn output_markdown(results: &[((&'static str, EndpointStatus, EndpointStatus), u32)]) {
    let mut builder = Builder::default();

    builder.set_header(["RPC Method", "Forest", "Lotus"]);

    for ((method, forest_status, lotus_status), n) in results {
        builder.push_record([
            if *n > 1 {
                format!("{} ({})", method, n)
            } else {
                format!("{}", method)
            },
            format!("{:?}", forest_status),
            format!("{:?}", lotus_status),
        ]);
    }

    let table = builder.build().with(Style::markdown()).to_string();

    println!("{}", table);
}
