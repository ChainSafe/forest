// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_api::progress_api::*;
use jsonrpc_v2::Error;

use crate::rpc_client::call;

pub async fn get_progress(
    params: GetProgressParams,
    auth_token: &Option<String>,
) -> Result<GetProgressResult, Error> {
    call(GET_PROGRESS, params, auth_token).await
}
