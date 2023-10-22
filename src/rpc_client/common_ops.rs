// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_api::{common_api::*, data_types::APIVersion};
use chrono::{DateTime, Utc};
use jsonrpc_v2::Error;

use crate::rpc_client::call;

use super::{ApiInfo, API_INFO};

impl ApiInfo {
    pub async fn version(&self) -> Result<APIVersion, Error> {
        self.call(VERSION, ()).await
    }

    pub async fn start_time(&self) -> Result<DateTime<Utc>, Error> {
        self.call(START_TIME, ()).await
    }

    pub async fn shutdown(&self) -> Result<(), Error> {
        self.call(SHUTDOWN, ()).await
    }
}

pub async fn version(
    (): VersionParams,
    auth_token: &Option<String>,
) -> Result<VersionResult, Error> {
    API_INFO
        .clone()
        .set_token(auth_token.clone())
        .version()
        .await
}

pub async fn shutdown(
    (): ShutdownParams,
    auth_token: &Option<String>,
) -> Result<ShutdownResult, Error> {
    call(SHUTDOWN, (), auth_token).await
}

pub async fn start_time(auth_token: &Option<String>) -> Result<StartTimeResult, Error> {
    call(START_TIME, (), auth_token).await
}
