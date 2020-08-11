// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use auth::*;
use jsonrpc_v2::{Error as JsonRpcError, Params};

/// RPC call to create a new JWT Token
pub(crate) async fn auth_new(
    Params(params): Params<(Vec<String>,)>,
) -> Result<String, JsonRpcError> {
    let (perms,) = params;
    let token = create_token(perms)?;
    Ok(token)
}

/// RPC call to verify JWT Token and return the token's permissions
pub(crate) async fn auth_verify(
    Params(params): Params<(String,)>,
) -> Result<Vec<String>, JsonRpcError> {
    let (token,) = params;
    let perms = verify_token(&token)?;
    Ok(perms)
}
