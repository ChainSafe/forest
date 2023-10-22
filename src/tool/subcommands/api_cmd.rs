// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashMap;
use clap::Subcommand;
use futures::Future;
use std::str::FromStr;

use crate::blocks::Tipset;
use crate::blocks::TipsetKeys;
use crate::rpc_api::chain_api::*;
use crate::rpc_api::common_api::*;
use crate::rpc_client::ApiInfo;

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

struct ApiStatus {
    forest_api: ApiInfo,
    lotus_api: ApiInfo,
    forest: HashMap<&'static str, EndpointStatus>,
    lotus: HashMap<&'static str, EndpointStatus>,
}

impl ApiStatus {
    fn new(forest_api: ApiInfo, lotus_api: ApiInfo) -> Self {
        ApiStatus {
            forest_api,
            lotus_api,
            forest: HashMap::default(),
            lotus: HashMap::default(),
        }
    }

    // Verify that both Forest and Lotus uses the same JSON schema for requests
    // and responses. The Forest response does not have to contain the same data
    // as Lotus' as long as the format is the same.
    async fn basic_check<T, Fut>(&mut self, ident: &'static str, call: impl Fn(ApiInfo) -> Fut)
    where
        Fut: Future<Output = Result<T, jsonrpc_v2::Error>>,
    {
        let forest_resp = call(self.forest_api.clone()).await;
        let lotus_resp = call(self.lotus_api.clone()).await;

        let forest_status = forest_resp
            .err()
            .map_or(EndpointStatus::Valid, EndpointStatus::from_json_error);
        let lotus_status = lotus_resp
            .err()
            .map_or(EndpointStatus::Valid, EndpointStatus::from_json_error);

        dbg!(ident);
        self.forest.insert(ident, dbg!(forest_status));
        self.lotus.insert(ident, dbg!(lotus_status));
    }

    async fn validate_check<T, Fut>(
        &mut self,
        ident: &'static str,
        call: impl Fn(ApiInfo) -> Fut,
        validate: impl Fn(T, T) -> bool,
    ) where
        Fut: Future<Output = Result<T, jsonrpc_v2::Error>>,
    {
        let forest_resp = call(self.forest_api.clone()).await;
        let lotus_resp = call(self.lotus_api.clone()).await;

        dbg!(ident);
        match (forest_resp, lotus_resp) {
            (Ok(forest), Ok(lotus)) => {
                if validate(forest, lotus) {
                    self.forest.insert(ident, dbg!(EndpointStatus::Valid));
                } else {
                    self.forest
                        .insert(ident, dbg!(EndpointStatus::InvalidResponse));
                }
                self.lotus.insert(ident, dbg!(EndpointStatus::Valid));
            }
            (forest_resp, lotus_resp) => {
                let forest_status = forest_resp
                    .err()
                    .map_or(EndpointStatus::Valid, EndpointStatus::from_json_error);
                let lotus_status = lotus_resp
                    .err()
                    .map_or(EndpointStatus::Valid, EndpointStatus::from_json_error);

                dbg!(ident);
                self.forest.insert(ident, dbg!(forest_status));
                self.lotus.insert(ident, dbg!(lotus_status));
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum EndpointStatus {
    Missing,
    InvalidRequest,
    InternalServerError,
    InvalidJSON,
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
        dbg!(&err_message(&err));
        dbg!(&err_code(&err));
        if err_code(&err) == err_code(&jsonrpc_v2::Error::INVALID_REQUEST) {
            EndpointStatus::InvalidRequest
        } else if err_code(&err) == err_code(&jsonrpc_v2::Error::METHOD_NOT_FOUND) {
            EndpointStatus::Missing
        } else if err_code(&err) == err_code(&jsonrpc_v2::Error::INVALID_REQUEST) {
            EndpointStatus::InvalidJSON
        } else if err_code(&err) == err_code(&jsonrpc_v2::Error::PARSE_ERROR) {
            EndpointStatus::InvalidResponse
        } else {
            EndpointStatus::InternalServerError
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

async fn compare_apis(forest: ApiInfo, lotus: ApiInfo) -> anyhow::Result<()> {
    let shared_tipset = youngest_tipset(&forest, &lotus).await?;
    let shared_block = shared_tipset.min_ticket_block();
    dbg!(shared_block.epoch());

    let mut status = ApiStatus::new(forest, lotus);

    status
        .basic_check(VERSION, |api| async move { api.version().await })
        .await;
    status
        .basic_check(START_TIME, |api| async move { api.start_time().await })
        .await;
    status
        .validate_check(
            CHAIN_HEAD,
            |api| async move { api.chain_head().await },
            |forest, lotus| forest.epoch().abs_diff(lotus.epoch()) < 10,
        )
        .await;

    status
        .validate_check(
            CHAIN_HEAD,
            |api| async move { api.chain_get_block(*shared_block.cid()).await },
            |forest, lotus| forest == lotus,
        )
        .await;

    status
        .validate_check(
            CHAIN_GET_TIPSET_BY_HEIGHT,
            |api| async move {
                api.chain_get_tipset_by_height(shared_block.epoch(), TipsetKeys::default())
                    .await
            },
            |forest, lotus| forest == lotus,
        )
        .await;

    status
        .validate_check(
            CHAIN_GET_GENESIS,
            |api| async move { api.chain_get_genesis().await },
            |forest, lotus| forest == lotus,
        )
        .await;
    // status
    //     .basic_check(START_TIME, |api| async move { api.shutdown().await })
    //     .await;
    Ok(())
}
