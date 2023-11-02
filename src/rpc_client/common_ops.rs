// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_api::{
    common_api::{DISCOVER, SESSION, SHUTDOWN, START_TIME, VERSION},
    data_types::{APIVersion, DiscoverResult},
};
use chrono::{DateTime, Utc};

use super::{ApiInfo, JsonRpcError, RpcRequest};

impl ApiInfo {
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

    pub fn session_req() -> RpcRequest<String> {
        RpcRequest::new(SESSION, ())
    }
}
