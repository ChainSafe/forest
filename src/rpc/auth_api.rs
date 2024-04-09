// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::auth::*;
use crate::lotus_json::lotus_json_with_self;
use crate::lotus_json::LotusJson;
use crate::rpc::{ApiVersion, Ctx, JsonRpcError, RpcMethod};
use anyhow::Result;
use chrono::Duration;
use fvm_ipld_blockstore::Blockstore;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DurationSeconds};

macro_rules! for_each_method {
    ($callback:ident) => {
        $callback!(crate::rpc::auth_api::AuthNew);
        $callback!(crate::rpc::auth_api::AuthVerify);
    };
}
pub(crate) use for_each_method;

/// RPC call to create a new JWT Token
pub enum AuthNew {}
impl RpcMethod<1> for AuthNew {
    const NAME: &'static str = "Filecoin.AuthNew";
    const PARAM_NAMES: [&'static str; 1] = ["params"];
    const API_VERSION: ApiVersion = ApiVersion::V0;
    type Params = (AuthNewParams,);
    type Ok = LotusJson<Vec<u8>>;
    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (params,): Self::Params,
    ) -> Result<Self::Ok, JsonRpcError> {
        let ks = ctx.keystore.read().await;
        let ki = ks.get(JWT_IDENTIFIER)?;
        let token = create_token(params.perms, ki.private_key(), params.token_exp)?;
        Ok(LotusJson(token.as_bytes().to_vec()))
    }
}

pub enum AuthVerify {}
impl RpcMethod<1> for AuthVerify {
    const NAME: &'static str = "Filecoin.AuthVerify";
    const PARAM_NAMES: [&'static str; 1] = ["header_raw"];
    const API_VERSION: ApiVersion = ApiVersion::V0;
    type Params = (String,);
    type Ok = Vec<String>;
    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (header_raw,): Self::Params,
    ) -> Result<Self::Ok, JsonRpcError> {
        let ks = ctx.keystore.read().await;
        let token = header_raw.trim_start_matches("Bearer ");
        let ki = ks.get(JWT_IDENTIFIER)?;
        let perms = verify_token(token, ki.private_key())?;
        Ok(perms)
    }
}

#[serde_as]
#[derive(Deserialize, Serialize)]
pub struct AuthNewParams {
    pub perms: Vec<String>,
    #[serde_as(as = "DurationSeconds<i64>")]
    pub token_exp: Duration,
}
lotus_json_with_self!(AuthNewParams);

/// `#[derive(JsonSchema)]` doesn't play nicely with [`serde_as`].
///
/// The correct solution is `token_exp: u64`, but the auth tests use negative
/// durations, so accept the tech debt for this for now
impl JsonSchema for AuthNewParams {
    fn schema_name() -> String {
        "AuthNewParams".into()
    }

    fn json_schema(gen: &mut schemars::gen::SchemaGenerator) -> schemars::schema::Schema {
        #[derive(JsonSchema)]
        #[allow(dead_code)]
        struct Helper {
            perms: Vec<String>,
            token_exp: i64,
        }
        Helper::json_schema(gen)
    }
}
