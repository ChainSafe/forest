// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use jsonrpc_v2::Error as JsonRpcError;
use rpc_api::auth_api::*;

use crate::call;

/// Creates a new JWT Token
pub async fn auth_new(perm: AuthNewParams) -> Result<AuthNewResult, JsonRpcError> {
    call(AUTH_NEW, perm).await
}

pub async fn auth_verify(token: AuthVerifyParams) -> Result<AuthVerifyResult, JsonRpcError> {
    call(AUTH_VERIFY, (token,)).await
}
