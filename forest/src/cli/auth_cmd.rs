// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{handle_rpc_err, print_rpc_res, Config};
use forest_libp2p::{Multiaddr, Protocol};
use rpc_client::{auth_new, DEFAULT_HOST};
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
    pub async fn run(&self, cfg: Config) {
        match self {
            Self::CreateToken { perm } => {
                let perm: String = perm.parse().unwrap();
                print_rpc_res(auth_new(perm).await);
            }
            Self::ApiInfo { perm } => {
                let perm: String = perm.parse().unwrap();
                match auth_new(perm).await {
                    Ok(token) => {
                        let mut addr = Multiaddr::empty();
                        addr.push(Protocol::Ip4(DEFAULT_HOST.parse().unwrap()));
                        addr.push(Protocol::Tcp(cfg.rpc_port.parse().unwrap()));
                        addr.push(Protocol::Http);
                        println!("FULLNODE_API_INFO=\"{}:{}\"", token, addr.to_string());
                    }
                    Err(e) => handle_rpc_err(e),
                };
            }
        }
    }
}
