// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_api::auth_api::*;
use jsonrpc_v2::Error as JsonRpcError;

use crate::rpc_client::call;

/// Creates a new JWT Token
pub async fn auth_new(
    perm: AuthNewParams,
    auth_token: &Option<String>,
) -> Result<AuthNewResult, JsonRpcError> {
    call(AUTH_NEW, perm, auth_token).await
}

pub async fn auth_verify(
    token: AuthVerifyParams,
    auth_token: &Option<String>,
) -> Result<AuthVerifyResult, JsonRpcError> {
    call(AUTH_VERIFY, (token,), auth_token).await
}
