// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_api::{common_api::*, data_types::APIVersion};
use chrono::{DateTime, Utc};
use jsonrpc_v2::Error;

use super::{ApiInfo, RpcRequest};

impl ApiInfo {
    pub async fn version(&self) -> Result<APIVersion, Error> {
        self.call_req(Self::version_req()).await
    }

    pub fn version_req() -> RpcRequest<APIVersion> {
        RpcRequest::new(VERSION, ())
    }

    pub async fn start_time(&self) -> Result<DateTime<Utc>, Error> {
        self.call_req(Self::start_time_req()).await
    }

    pub fn start_time_req() -> RpcRequest<DateTime<Utc>> {
        RpcRequest::new(START_TIME, ())
    }

    pub async fn shutdown(&self) -> Result<(), Error> {
        self.call_req(Self::shutdown_req()).await
    }

    pub fn shutdown_req() -> RpcRequest<()> {
        RpcRequest::new(SHUTDOWN, ())
    }
}
