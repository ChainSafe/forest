// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::auth::*;
use crate::rpc_client::{auth_new_req, ApiInfo, API_INFO};
use chrono::Duration;
use clap::Subcommand;
use jsonrpc_v2::Error as JsonRpcError;
use std::str::FromStr;

use super::{handle_rpc_err, print_rpc_res_bytes};

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

fn process_perms(perm: String) -> Result<Vec<String>, JsonRpcError> {
    Ok(match perm.as_str() {
        "admin" => ADMIN,
        "sign" => SIGN,
        "write" => WRITE,
        "read" => READ,
        _ => return Err(JsonRpcError::INVALID_PARAMS),
    }
    .iter()
    .map(ToString::to_string)
    .collect())
}

impl AuthCommands {
    pub async fn run(self, api: ApiInfo) -> anyhow::Result<()> {
        match self {
            Self::CreateToken { perm, expire_in } => {
                let perm: String = perm.parse()?;
                let perms = process_perms(perm).map_err(handle_rpc_err)?;
                let token_exp = Duration::from_std(expire_in.into())?;
                print_rpc_res_bytes(api.call_req(auth_new_req(perms, token_exp)).await)
            }
            Self::ApiInfo { perm, expire_in } => {
                let perm: String = perm.parse()?;
                let perms = process_perms(perm).map_err(handle_rpc_err)?;
                let token_exp = Duration::from_std(expire_in.into())?;
                let token = api
                    .call_req(auth_new_req(perms, token_exp))
                    .await
                    .map_err(handle_rpc_err)?;
                let addr = API_INFO.multiaddr.to_owned();
                println!(
                    "FULLNODE_API_INFO=\"{}:{}\"",
                    String::from_utf8(token).map_err(|e| handle_rpc_err(e.into()))?,
                    addr
                );
                Ok(())
            }
        }
    }
}
