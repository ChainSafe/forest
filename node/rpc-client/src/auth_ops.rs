// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::client::filecoin_rpc;
use auth::*;
use jsonrpc_v2::Error as JsonRpcError;

/// Creates a new JWT Token
pub async fn auth_new(perm: String) -> Result<String, JsonRpcError> {
    let perms = match perm.as_str() {
        "admin" => ADMIN.to_owned(),
        "sign" => SIGN.to_owned(),
        "write" => WRITE.to_owned(),
        "read" => READ.to_owned(),
        _ => {
            return Err(JsonRpcError::INVALID_PARAMS);
        }
    };

    let ret: Vec<u8> = filecoin_rpc::auth_new((perms,)).await?;

    Ok(String::from_utf8(ret)?)
}
