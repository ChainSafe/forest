// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{ApiInfo, RpcRequest, ServerError};
use crate::rpc::common_api::{DISCOVER, SESSION, SHUTDOWN, START_TIME, VERSION};
use crate::rpc::types::{APIVersion, DiscoverResult};
use chrono::{DateTime, Utc};

impl ApiInfo {
    pub fn version_req() -> RpcRequest<APIVersion> {
        RpcRequest::new(VERSION, ())
    }

    pub async fn start_time(&self) -> Result<DateTime<Utc>, ServerError> {
        self.call(Self::start_time_req()).await
    }

    pub fn start_time_req() -> RpcRequest<DateTime<Utc>> {
        RpcRequest::new(START_TIME, ())
    }

    pub async fn shutdown(&self) -> Result<(), ServerError> {
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
