// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_api::{common_api::*, data_types::APIVersion};
use chrono::{DateTime, Utc};
use jsonrpc_v2::Error;

use crate::rpc_client::call;

use super::{call_req, ApiInfo, RpcRequest, API_INFO};

impl ApiInfo {
    pub async fn version(&self) -> Result<APIVersion, Error> {
        self.call_req(version_req()).await
    }

    pub async fn start_time(&self) -> Result<DateTime<Utc>, Error> {
        self.call_req(start_time_req()).await
    }

    pub async fn shutdown(&self) -> Result<(), Error> {
        self.call(SHUTDOWN, ()).await
    }
}

pub fn version_req() -> RpcRequest<APIVersion> {
    RpcRequest::new(VERSION, ())
}

pub fn shutdown_req() -> RpcRequest<()> {
    RpcRequest::new(SHUTDOWN, ())
}

pub async fn start_time(auth_token: &Option<String>) -> Result<StartTimeResult, Error> {
    call(START_TIME, (), auth_token).await
}

pub fn start_time_req() -> RpcRequest<DateTime<Utc>> {
    RpcRequest::new(START_TIME, ())
}
