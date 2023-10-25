// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::lotus_json::lotus_json_with_self;
use crate::rpc_api::{common_api::*, data_types::APIVersion};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{ApiInfo, JsonRpcError, RpcRequest};

impl ApiInfo {
    // Current unused
    // pub async fn version(&self) -> Result<APIVersion, JsonRpcError> {
    //     self.call_req_e(Self::version_req()).await
    // }

    pub fn version_req() -> RpcRequest<APIVersion> {
        RpcRequest::new(VERSION, ())
    }

    pub async fn start_time(&self) -> Result<DateTime<Utc>, JsonRpcError> {
        self.call(Self::start_time_req()).await
    }

    pub fn start_time_req() -> RpcRequest<DateTime<Utc>> {
        RpcRequest::new(START_TIME, ())
    }

    pub async fn shutdown(&self) -> Result<(), JsonRpcError> {
        self.call(Self::shutdown_req()).await
    }

    pub fn shutdown_req() -> RpcRequest<()> {
        RpcRequest::new(SHUTDOWN, ())
    }

    pub fn discover_req() -> RpcRequest<DiscoverResult> {
        RpcRequest::new(DISCOVER, ())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscoverResult {
    info: DiscoverInfo,
    methods: Vec<DiscoverMethod>,
    openrpc: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverMethod {
    deprecated: bool,
    description: String,
    external_docs: DiscoverDocs,
    name: String,
    param_structure: String,
    params: Value,
    // result
    summary: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscoverDocs {
    description: String,
    url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscoverInfo {
    title: String,
    version: String,
}

lotus_json_with_self!(DiscoverResult, DiscoverMethod, DiscoverDocs, DiscoverInfo);
