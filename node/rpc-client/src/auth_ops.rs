// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::client::Filecoin;
use jsonrpc_v2::Error as JsonRpcError;
use jsonrpsee::raw::RawClient;
use jsonrpsee::transport::http::HttpTransportClient as HTC;

lazy_static! {
    pub static ref ADMIN: Vec<String> = vec![
        "read".to_string(),
        "write".to_string(),
        "sign".to_string(),
        "admin".to_string()
    ];
    pub static ref SIGN: Vec<String> =
        vec!["read".to_string(), "write".to_string(), "sign".to_string()];
    pub static ref WRITE: Vec<String> = vec!["read".to_string(), "write".to_string()];
    pub static ref READ: Vec<String> = vec!["read".to_string()];
}

/// Returns a block with specified CID fom chain via RPC
pub async fn auth_new(client: &mut RawClient<HTC>, perm: String) -> Result<String, JsonRpcError> {
    let ret: String = match perm.as_str() {
        "admin" => Filecoin::auth_new(client, ADMIN.clone()).await?,
        "sign" => Filecoin::auth_new(client, SIGN.clone()).await?,
        "write" => Filecoin::auth_new(client, WRITE.clone()).await?,
        "read" => Filecoin::auth_new(client, READ.clone()).await?,
        _ => return Err(JsonRpcError::INVALID_PARAMS),
    };
    Ok(ret)
}
