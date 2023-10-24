// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_api::{common_api::*, data_types::APIVersion};
use chrono::{DateTime, Utc};

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
}
