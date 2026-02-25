// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    KeyStore,
    auth::*,
    lotus_json::lotus_json_with_self,
    rpc::{ApiPaths, Ctx, Permission, RpcMethod, ServerError},
};
use anyhow::Result;
use chrono::Duration;
use enumflags2::BitFlags;
use fvm_ipld_blockstore::Blockstore;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::{DurationSeconds, serde_as};

/// RPC call to create a new JWT Token
pub enum AuthNew {}

impl AuthNew {
    pub fn create_token(
        keystore: &KeyStore,
        token_exp: Duration,
        permissions: Vec<String>,
    ) -> anyhow::Result<String> {
        let ki = keystore.get(JWT_IDENTIFIER)?;
        Ok(create_token(permissions, ki.private_key(), token_exp)?)
    }
}

impl RpcMethod<2> for AuthNew {
    const NAME: &'static str = "Filecoin.AuthNew";
    const N_REQUIRED_PARAMS: usize = 1;
    // Note: Lotus does not support the optional `expiration_secs` parameter
    const PARAM_NAMES: [&'static str; 2] = ["permissions", "expiration_secs"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Admin;
    type Params = (Vec<String>, Option<i64>);
    type Ok = Vec<u8>;
    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (permissions, expiration_secs): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ks = ctx.keystore.read();
        // Lotus admin tokens do not expire but Forest requires all JWT tokens to
        // have an expiration date. So we set the expiration date to 100 years in
        // the future to match user-visible behavior of Lotus.
        let token_exp = expiration_secs
            .map(chrono::Duration::seconds)
            .unwrap_or_else(|| chrono::Duration::days(365 * 100));
        let token = Self::create_token(&ks, token_exp, permissions)?;
        Ok(token.as_bytes().to_vec())
    }
}

pub enum AuthVerify {}
impl RpcMethod<1> for AuthVerify {
    const NAME: &'static str = "Filecoin.AuthVerify";
    const PARAM_NAMES: [&'static str; 1] = ["header_raw"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    type Params = (String,);
    type Ok = Vec<String>;
    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (header_raw,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ks = ctx.keystore.read();
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
