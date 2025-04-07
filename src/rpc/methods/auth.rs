// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::auth::*;
use crate::lotus_json::lotus_json_with_self;
use crate::rpc::{Ctx, Permission, RpcMethod, ServerError};
use anyhow::Result;
use chrono::Duration;
use fvm_ipld_blockstore::Blockstore;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::{DurationSeconds, serde_as};

/// RPC call to create a new JWT Token
pub enum AuthNew {}
impl RpcMethod<2> for AuthNew {
    const NAME: &'static str = "Filecoin.AuthNew";
    const N_REQUIRED_PARAMS: usize = 1;
    // Note: Lotus does not support the optional `expiration_secs` parameter
    const PARAM_NAMES: [&'static str; 2] = ["permissions", "expiration_secs"];
    const PERMISSION: Permission = Permission::Admin;
    type Params = (Vec<String>, Option<i64>);
    type Ok = Vec<u8>;
    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (permissions, expiration_secs): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ks = ctx.keystore.read().await;
        let ki = ks.get(JWT_IDENTIFIER)?;
        let token = create_token(
            permissions,
            ki.private_key(),
            // default to 24h
            chrono::Duration::seconds(expiration_secs.unwrap_or(60 * 60 * 24)),
        )?;
        Ok(token.as_bytes().to_vec())
    }
}

pub enum AuthVerify {}
impl RpcMethod<1> for AuthVerify {
    const NAME: &'static str = "Filecoin.AuthVerify";
    const PARAM_NAMES: [&'static str; 1] = ["header_raw"];
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

impl AuthNewParams {
    pub fn process_perms(perm: String) -> Result<Vec<String>, ServerError> {
        Ok(match perm.to_lowercase().as_str() {
            "admin" => ADMIN,
            "sign" => SIGN,
            "write" => WRITE,
            "read" => READ,
            _ => return Err(ServerError::invalid_params("unknown permission", None)),
        }
        .iter()
        .map(ToString::to_string)
        .collect())
    }
}

impl From<AuthNewParams> for (Vec<String>, Option<i64>) {
    fn from(value: AuthNewParams) -> Self {
        (value.perms, Some(value.token_exp.num_seconds()))
    }
}
