// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_api::auth_api::*;
use chrono::Duration;
use jsonrpc_v2::Error as JsonRpcError;

use crate::rpc_client::call;

use super::RpcRequest;

/// Creates a new JWT Token
pub async fn auth_new(
    perm: AuthNewParams,
    auth_token: &Option<String>,
) -> Result<AuthNewResult, JsonRpcError> {
    call(AUTH_NEW, perm, auth_token).await
}

pub fn auth_new_req(perms: Vec<String>, token_exp: Duration) -> RpcRequest<Vec<u8>> {
    RpcRequest::new(AUTH_NEW, AuthNewParams { perms, token_exp })
}
