// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::{self, auth::AuthNewParams, prelude::*};
use crate::utils;
use clap::Subcommand;
use libp2p::multiaddr;

use super::print_rpc_res_bytes;

#[derive(Debug, Subcommand)]
pub enum AuthCommands {
    /// Create a new Authentication token with given permission
    CreateToken {
        /// Permission to assign to the token, one of: read, write, sign, admin
        #[arg(short, long)]
        perm: String,
        /// Token is revoked after this duration
        #[arg(long, default_value = "2 months", value_parser = utils::parse_duration)]
        expire_in: std::time::Duration,
    },
    /// Get RPC API Information
    ApiInfo {
        /// permission to assign the token, one of: read, write, sign, admin
        #[arg(short, long)]
        perm: String,
        /// Token is revoked after this duration
        #[arg(long, default_value = "2 months", value_parser = utils::parse_duration)]
        expire_in: std::time::Duration,
    },
}

impl AuthCommands {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::CreateToken { perm, expire_in } => {
                let perm: String = perm.parse()?;
                let perms = AuthNewParams::process_perms(perm)?;
                let token_exp_secs = i64::try_from(expire_in.as_secs())?;
                let res = AuthNew::call(
                    &client,
                    AuthNewParams {
                        perms,
                        token_exp_secs,
                    }
                    .into(),
                )
                .await?;
                print_rpc_res_bytes(res)
            }
            Self::ApiInfo { perm, expire_in } => {
                let perm: String = perm.parse()?;
                let perms = AuthNewParams::process_perms(perm)?;
                let token_exp_secs = i64::try_from(expire_in.as_secs())?;
                let token = String::from_utf8(
                    AuthNew::call(
                        &client,
                        AuthNewParams {
                            perms,
                            token_exp_secs,
                        }
                        .into(),
                    )
                    .await?,
                )?;
                let addr = multiaddr::from_url(client.base_url().as_str())?;
                println!("FULLNODE_API_INFO=\"{token}:{addr}\"");
                Ok(())
            }
        }
    }
}
