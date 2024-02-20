// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::auth::*;
use crate::lotus_json::LotusJson;
use crate::rpc::error::JsonRpcError;
use crate::rpc_api::{
    auth_api::*,
    data_types::{Data, RPCState},
};
use anyhow::Result;
use fvm_ipld_blockstore::Blockstore;
use jsonrpsee::types::Params;

/// RPC call to create a new JWT Token
pub async fn auth_new<DB: Blockstore>(
    params: Params<'_>,
    data: Data<RPCState<DB>>,
) -> Result<LotusJson<Vec<u8>>, JsonRpcError> {
    let auth_params: AuthNewParams = params.parse()?;

    let ks = data.keystore.read().await;
    let ki = ks.get(JWT_IDENTIFIER)?;
    let token = create_token(auth_params.perms, ki.private_key(), auth_params.token_exp)?;
    Ok(LotusJson(token.as_bytes().to_vec()))
}

/// RPC call to verify JWT Token and return the token's permissions
pub async fn auth_verify<DB>(
    params: Params<'_>,
    data: Data<RPCState<DB>>,
) -> Result<Vec<String>, JsonRpcError>
where
    DB: Blockstore,
{
    let (header_raw,): (String,) = params.parse()?;

    let ks = data.keystore.read().await;
    let token = header_raw.trim_start_matches("Bearer ");
    let ki = ks.get(JWT_IDENTIFIER)?;
    let perms = verify_token(token, ki.private_key())?;
    Ok(perms)
}
