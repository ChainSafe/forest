// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_client::ApiInfo;
use crate::{
    auth::*,
    rpc::{self, auth::AuthNewParams, prelude::*},
};
use chrono::Duration;
use clap::Subcommand;
use std::str::FromStr;

use super::print_rpc_res_bytes;

#[derive(Debug, Subcommand)]
pub enum AuthCommands {
    /// Create a new Authentication token with given permission
    CreateToken {
        /// Permission to assign to the token, one of: read, write, sign, admin
        #[arg(short, long)]
        perm: String,
        /// Token is revoked after this duration
        #[arg(long, default_value_t = humantime::Duration::from_str("2 months").expect("infallible"))]
        expire_in: humantime::Duration,
    },
    /// Get RPC API Information
    ApiInfo {
        /// permission to assign the token, one of: read, write, sign, admin
        #[arg(short, long)]
        perm: String,
        /// Token is revoked after this duration
        #[arg(long, default_value_t = humantime::Duration::from_str("2 months").expect("infallible"))]
        expire_in: humantime::Duration,
    },
}

fn process_perms(perm: String) -> Result<Vec<String>, rpc::ServerError> {
    Ok(match perm.as_str() {
        "admin" => ADMIN,
        "sign" => SIGN,
        "write" => WRITE,
        "read" => READ,
        _ => return Err(rpc::ServerError::invalid_params("unknown permission", None)),
    }
    .iter()
    .map(ToString::to_string)
    .collect())
}

impl AuthCommands {
    pub async fn run(self, api: ApiInfo) -> anyhow::Result<()> {
        let client = rpc::Client::from(api.clone());
        match self {
            Self::CreateToken { perm, expire_in } => {
                let perm: String = perm.parse()?;
                let perms = process_perms(perm)?;
                let token_exp = Duration::from_std(expire_in.into())?;
                let res = AuthNew::call(&client, (AuthNewParams { perms, token_exp },))
                    .await?
                    .into_inner();
                print_rpc_res_bytes(res)
            }
            Self::ApiInfo { perm, expire_in } => {
                let perm: String = perm.parse()?;
                let perms = process_perms(perm)?;
                let token_exp = Duration::from_std(expire_in.into())?;
                let token = AuthNew::call(&client, (AuthNewParams { perms, token_exp },))
                    .await?
                    .into_inner();
                let new_api = api.set_token(Some(String::from_utf8(token)?));
                println!("FULLNODE_API_INFO=\"{}\"", new_api);
                Ok(())
            }
        }
    }
}
