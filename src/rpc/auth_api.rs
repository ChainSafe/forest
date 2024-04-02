// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::auth::*;
use crate::lotus_json::lotus_json_with_self;
use crate::lotus_json::LotusJson;
use crate::rpc::error::JsonRpcError;
use crate::rpc::Ctx;
use anyhow::Result;
use chrono::Duration;
use fvm_ipld_blockstore::Blockstore;
use jsonrpsee::types::Params;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DurationSeconds};

pub const AUTH_NEW: &str = "Filecoin.AuthNew";
#[serde_as]
#[derive(Deserialize, Serialize)]
pub struct AuthNewParams {
    pub perms: Vec<String>,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub token_exp: Duration,
}
lotus_json_with_self!(AuthNewParams);

pub const AUTH_VERIFY: &str = "Filecoin.AuthVerify";

/// RPC call to create a new JWT Token
pub async fn auth_new<DB: Blockstore>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<Vec<u8>>, JsonRpcError> {
    let auth_params: AuthNewParams = params.parse()?;

    let ks = data.keystore.read().await;
    let ki = ks.get(JWT_IDENTIFIER)?;
    let token = create_token(auth_params.perms, ki.private_key(), auth_params.token_exp)?;
    Ok(LotusJson(token.as_bytes().to_vec()))
}

/// RPC call to verify JWT Token and return the token's permissions
pub async fn auth_verify<DB>(params: Params<'_>, data: Ctx<DB>) -> Result<Vec<String>, JsonRpcError>
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
