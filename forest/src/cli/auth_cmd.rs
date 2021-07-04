// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{get_config, handle_rpc_err, print_rpc_res};
use rpc_client::auth_new;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub enum AuthCommands {
    /// Create a new Authentication token with given permission
    #[structopt(about = "<String> Create Authentication token with given permission")]
    CreateToken {
        #[structopt(
            short,
            long,
            help = "permission to assign to the token, one of: read, write, sign, admin"
        )]
        perm: String,
    },
    #[structopt(about = "Get RPC API information")]
    ApiInfo {
        #[structopt(
            short,
            long,
            help = "permission to assign the token, one of: read, write, sign, admin"
        )]
        perm: String,
    },
}

impl AuthCommands {
    pub async fn run(&self) {
        match self {
            Self::CreateToken { perm } => {
                let perm: String = perm.parse().unwrap();
                print_rpc_res(auth_new(perm).await);
            }
            Self::ApiInfo { perm } => {
                let perm: String = perm.parse().unwrap();
                match auth_new(perm).await {
                    Ok(token) => {
                        let cfg = get_config().await;
                        let multiaddr = todo!();
                        format!("FULLNODE_API_INFO=\"{}:{}\"", token, multiaddr.to_string())
                    }
                    Err(e) => handle_rpc_err(e),
                };
            }
        }
    }
}
