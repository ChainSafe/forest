// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::client::filecoin_rpc;
use auth::*;
use jsonrpc_v2::Error as JsonRpcError;

fn match_perms(perm: String) -> Result<Vec<String>, JsonRpcError> {
    match perm.as_str() {
        "admin" => Ok(ADMIN.to_owned()),
        "sign" => Ok(SIGN.to_owned()),
        "write" => Ok(WRITE.to_owned()),
        "read" => Ok(READ.to_owned()),
        _ => Err(JsonRpcError::INVALID_PARAMS),
    }
}

/// Creates a new JWT Token
pub async fn auth_new(perm: String) -> Result<String, JsonRpcError> {
    let perms = match_perms(perm)?;
    let ret: Vec<u8> = filecoin_rpc::auth_new((perms,)).await?;
    Ok(String::from_utf8(ret)?)
}

pub async fn auth_verify(token: String) -> Result<bool, JsonRpcError> {
    Ok(filecoin_rpc::auth_verify((token,)).await.is_ok())
}
