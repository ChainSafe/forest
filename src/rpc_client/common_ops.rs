// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_api::common_api::*;
use jsonrpc_v2::Error;

use crate::rpc_client::call;

pub async fn version(
    _: VersionParams,
    auth_token: &Option<String>,
) -> Result<VersionResult, Error> {
    call(VERSION, (), auth_token).await
}

pub async fn shutdown(
    _: ShutdownParams,
    auth_token: &Option<String>,
) -> Result<ShutdownResult, Error> {
    call(SHUTDOWN, (), auth_token).await
}

pub async fn start_time(auth_token: &Option<String>) -> Result<StartTimeResult, Error> {
    call(START_TIME, (), auth_token).await
}
