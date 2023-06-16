// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_rpc_api::common_api::*;
use jsonrpc_v2::Error;

use crate::call;

pub async fn version(
    params: VersionParams,
    auth_token: &Option<String>,
) -> Result<VersionResult, Error> {
    call(VERSION, params, auth_token).await
}

pub async fn shutdown(
    params: ShutdownParams,
    auth_token: &Option<String>,
) -> Result<ShutdownResult, Error> {
    call(SHUTDOWN, params, auth_token).await
}

pub async fn start_time(auth_token: &Option<String>) -> Result<StartTimeResult, Error> {
    call(START_TIME, (), auth_token).await
}
