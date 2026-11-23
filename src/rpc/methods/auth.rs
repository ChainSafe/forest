// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    KeyStore,
    auth::*,
    rpc::{ApiPaths, Ctx, Permission, RpcMethod, ServerError},
    utils::time::SignedDuration,
};
use anyhow::Result;
use enumflags2::BitFlags;

/// RPC call to create a new JWT Token
pub enum AuthNew {}

impl AuthNew {
    pub fn create_token(
        keystore: &KeyStore,
        lifetime: SignedDuration,
        permissions: Vec<String>,
    ) -> anyhow::Result<String> {
        let ki = keystore.get(JWT_IDENTIFIER)?;
        create_token(permissions, ki.private_key(), lifetime)
    }
}

impl RpcMethod<2> for AuthNew {
    const NAME: &'static str = "Filecoin.AuthNew";
    const N_REQUIRED_PARAMS: usize = 1;
    // Note: Lotus does not support the optional `expiration_secs` parameter
    const PARAM_NAMES: [&'static str; 2] = ["permissions", "expirationSecs"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Admin;
    const DESCRIPTION: &'static str =
        "Creates a new JWT authentication token with the given permissions.";
    type Params = (Vec<String>, Option<i64>);
    type Ok = Vec<u8>;
    async fn handle(
        ctx: Ctx,
        (permissions, expiration_secs): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ks = ctx.keystore.read();
        let lifetime =
            expiration_secs.map_or(NON_EXPIRING_TOKEN_LIFETIME, SignedDuration::from_secs);
        let token = Self::create_token(&ks, lifetime, permissions)?;
        Ok(token.as_bytes().to_vec())
    }
}

pub enum AuthVerify {}
impl RpcMethod<1> for AuthVerify {
    const NAME: &'static str = "Filecoin.AuthVerify";
    const PARAM_NAMES: [&'static str; 1] = ["token"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: &'static str =
        "Verifies a JWT authentication token and returns its permissions.";
    type Params = (String,);
    type Ok = Vec<String>;
    async fn handle(
        ctx: Ctx,
        (token,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ks = ctx.keystore.read();
        let ki = ks.get(JWT_IDENTIFIER)?;
        let perms = verify_token(&token, ki.private_key())?;
        Ok(perms)
    }
}

#[derive(Clone)]
pub struct AuthNewParams {
    pub perms: Vec<String>,
    pub lifetime_secs: i64,
}

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
        (value.perms, Some(value.lifetime_secs))
    }
}
