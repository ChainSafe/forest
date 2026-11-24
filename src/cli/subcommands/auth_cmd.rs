// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::{self, auth::AuthNewParams, prelude::*};
use crate::utils;
use clap::Subcommand;
use libp2p::multiaddr;
use std::time::Duration;

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
        expire_in: Duration,
    },
    /// Get RPC API Information
    ApiInfo {
        /// permission to assign the token, one of: read, write, sign, admin
        #[arg(short, long)]
        perm: String,
        /// Token is revoked after this duration
        #[arg(long, default_value = "2 months", value_parser = utils::parse_duration)]
        expire_in: Duration,
    },
}

impl AuthCommands {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::CreateToken { perm, expire_in } => {
                print_rpc_res_bytes(new_token(&client, perm, expire_in).await?)
            }
            Self::ApiInfo { perm, expire_in } => {
                let token = String::from_utf8(new_token(&client, perm, expire_in).await?)?;
                let addr = multiaddr::from_url(client.base_url().as_str())?;
                println!("FULLNODE_API_INFO=\"{token}:{addr}\"");
                Ok(())
            }
        }
    }
}

async fn new_token(
    client: &rpc::Client,
    perm: String,
    expire_in: Duration,
) -> anyhow::Result<Vec<u8>> {
    let perms = AuthNewParams::process_perms(perm)?;
    let lifetime_secs = i64::try_from(expire_in.as_secs())?;
    Ok(AuthNew::call(
        client,
        AuthNewParams {
            perms,
            lifetime_secs,
        }
        .into(),
    )
    .await?)
}
