// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::auth::*;
use crate::lotus_json::lotus_json_with_self;
use crate::rpc::{ApiPaths, Ctx, Permission, RpcMethod, ServerError};
use anyhow::Result;
use chrono::Duration;
use fvm_ipld_blockstore::Blockstore;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DurationSeconds};

/// RPC call to create a new JWT Token
pub enum AuthNew {}
impl RpcMethod<1> for AuthNew {
    const NAME: &'static str = "Filecoin.AuthNew";
    const PARAM_NAMES: [&'static str; 1] = ["params"];
    const API_PATHS: ApiPaths = ApiPaths::V0;
    const PERMISSION: Permission = Permission::Admin;
    type Params = (AuthNewParams,);
    type Ok = Vec<u8>;
    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (params,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ks = ctx.keystore.read().await;
        let ki = ks.get(JWT_IDENTIFIER)?;
        let token = create_token(params.perms, ki.private_key(), params.token_exp)?;
        Ok(token.as_bytes().to_vec())
    }
}

pub enum AuthVerify {}
impl RpcMethod<1> for AuthVerify {
    const NAME: &'static str = "Filecoin.AuthVerify";
    const PARAM_NAMES: [&'static str; 1] = ["header_raw"];
    const API_PATHS: ApiPaths = ApiPaths::V0;
    const PERMISSION: Permission = Permission::Read;
    type Params = (String,);
    type Ok = Vec<String>;
    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (header_raw,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ks = ctx.keystore.read().await;
        let token = header_raw.trim_start_matches("Bearer ");
        let ki = ks.get(JWT_IDENTIFIER)?;
        let perms = verify_token(token, ki.private_key())?;
        Ok(perms)
    }
}

#[serde_as]
#[derive(Clone, Deserialize, Serialize, JsonSchema)]
pub struct AuthNewParams {
    pub perms: Vec<String>,
    #[serde_as(as = "DurationSeconds<i64>")]
    #[schemars(with = "i64")]
    pub token_exp: Duration,
}
lotus_json_with_self!(AuthNewParams);
