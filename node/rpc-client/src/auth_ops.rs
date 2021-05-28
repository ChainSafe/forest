// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::client::filecoin_rpc;
use auth::*;
use jsonrpc_v2::Error as JsonRpcError;

/// Creates a new JWT Token
pub async fn auth_new(perm: String) -> Result<String, JsonRpcError> {
    let ret: Vec<u8> = match perm.as_str() {
        "admin" => {
            let perms: Vec<String> = ADMIN.iter().map(|s| s.to_string()).collect();
            filecoin_rpc::auth_new(perms).await?
        }
        "sign" => {
            let perms: Vec<String> = SIGN.iter().map(|s| s.to_string()).collect();
            filecoin_rpc::auth_new(perms).await?
        }
        "write" => {
            let perms: Vec<String> = WRITE.iter().map(|s| s.to_string()).collect();
            filecoin_rpc::auth_new(perms).await?
        }
        "read" => {
            let perms: Vec<String> = READ.iter().map(|s| s.to_string()).collect();
            filecoin_rpc::auth_new(perms).await?
        }
        _ => {
            return Err(JsonRpcError::INVALID_PARAMS);
        }
    };
    Ok(String::from_utf8(ret)?)
}
