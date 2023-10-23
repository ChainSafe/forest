// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashMap;
use clap::Subcommand;
use futures::Future;
use serde::de::DeserializeOwned;
use std::str::FromStr;

use crate::blocks::Tipset;
use crate::blocks::TipsetKeys;
use crate::lotus_json::HasLotusJson;
use crate::rpc_api::chain_api::*;
use crate::rpc_api::common_api::*;
use crate::rpc_client::chain_get_block_req;
use crate::rpc_client::chain_get_genesis_req;
use crate::rpc_client::chain_get_tipset_by_height_req;
use crate::rpc_client::chain_head_req;
use crate::rpc_client::common_ops;
use crate::rpc_client::start_time_req;
use crate::rpc_client::version_req;
use crate::rpc_client::ApiInfo;
use crate::rpc_client::RpcRequest;

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

#[derive(Debug, Clone)]
enum EndpointStatus {
    // RPC endpoint is missing (currently not reported correctly by either Forest nor Lotus)
    Missing,
    // Request isn't valid according to jsonrpc spec
    InvalidRequest,
    // Catch-all for errors on the node
    InternalServerError(String),
    // Unexpected JSON schema
    InvalidJSON,
    // Got response with the right JSON schema but it failed sanity checking
    InvalidResponse,
    Valid,
}

fn err_message(err: &jsonrpc_v2::Error) -> String {
    match err {
        jsonrpc_v2::Error::Full { message, .. } => String::from(message),
        jsonrpc_v2::Error::Provided { message, .. } => String::from(*message),
    }
}
fn err_code(err: &jsonrpc_v2::Error) -> i64 {
    match err {
        jsonrpc_v2::Error::Full { code, .. } => *code,
        jsonrpc_v2::Error::Provided { code, .. } => *code,
    }
}

impl EndpointStatus {
    fn from_json_error(err: jsonrpc_v2::Error) -> Self {
        // dbg!(&err_message(&err));
        // dbg!(&err_code(&err));
        if err_code(&err) == err_code(&jsonrpc_v2::Error::INVALID_REQUEST) {
            EndpointStatus::InvalidRequest
        } else if err_code(&err) == err_code(&jsonrpc_v2::Error::METHOD_NOT_FOUND) {
            EndpointStatus::Missing
        } else if err_code(&err) == err_code(&jsonrpc_v2::Error::INVALID_REQUEST) {
            EndpointStatus::InvalidJSON
        } else if err_code(&err) == err_code(&jsonrpc_v2::Error::PARSE_ERROR) {
            EndpointStatus::InvalidResponse
        } else {
            EndpointStatus::InternalServerError(err_message(&err))
        }
    }
}

fn handle_rpc_err(e: jsonrpc_v2::Error) -> anyhow::Error {
    match serde_json::to_string(&e) {
        Ok(err_msg) => anyhow::Error::msg(err_msg),
        Err(err) => err.into(),
    }
}

async fn youngest_tipset(forest: &ApiInfo, lotus: &ApiInfo) -> anyhow::Result<Tipset> {
    let t1 = forest.chain_head().await.map_err(handle_rpc_err)?;
    let t2 = lotus.chain_head().await.map_err(handle_rpc_err)?;
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
        let forest_resp = forest_api.call_req(self.request.clone()).await;
        let lotus_resp = lotus_api.call_req(self.request.clone()).await;

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
        RpcTest::basic(version_req()),
        RpcTest::basic(start_time_req()),
    ]
}

fn chain_tests(shared_tipset: &Tipset) -> Vec<RpcTest> {
    let shared_block = shared_tipset.min_ticket_block();

    vec![
        RpcTest::validate(chain_head_req(), |forest, lotus| {
            forest.epoch().abs_diff(lotus.epoch()) < 10
        }),
        RpcTest::identity(chain_get_block_req(*shared_block.cid())),
        RpcTest::identity(chain_get_tipset_by_height_req(
            shared_tipset.epoch(),
            TipsetKeys::default(),
        )),
        RpcTest::identity(chain_get_genesis_req()),
    ]
}

async fn compare_apis(forest: ApiInfo, lotus: ApiInfo) -> anyhow::Result<()> {
    let shared_tipset = youngest_tipset(&forest, &lotus).await?;
    let shared_block = shared_tipset.min_ticket_block();
    dbg!(shared_block.epoch());

    let mut tests = vec![];

    tests.extend(common_tests());
    tests.extend(chain_tests(&shared_tipset));

    for test in tests.into_iter() {
        eprintln!("Testing: {} ", test.request.method_name);
        eprintln!("Result: {:?}", test.run(&forest, &lotus).await);
    }

    Ok(())
}
