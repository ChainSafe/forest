// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::client::Filecoin;
use auth::*;
use jsonrpc_v2::Error as JsonRpcError;
use jsonrpsee::raw::RawClient;
use jsonrpsee::transport::http::HttpTransportClient as HTC;

/// Creates a new JWT Token
pub async fn auth_new(client: &mut RawClient<HTC>, perm: String) -> Result<String, JsonRpcError> {
    let ret: String = match perm.as_str() {
        "admin" => {
            let perms: Vec<String> = ADMIN.iter().map(|s| s.to_string()).collect();
            Filecoin::auth_new(client, perms).await?
        }
        "sign" => {
            let perms: Vec<String> = SIGN.iter().map(|s| s.to_string()).collect();
            Filecoin::auth_new(client, perms).await?
        }
        "write" => {
            let perms: Vec<String> = WRITE.iter().map(|s| s.to_string()).collect();
            Filecoin::auth_new(client, perms).await?
        }
        "read" => {
            let perms: Vec<String> = READ.iter().map(|s| s.to_string()).collect();
            Filecoin::auth_new(client, perms).await?
        }
        _ => return Err(JsonRpcError::INVALID_PARAMS),
    };
    Ok(ret)
}
