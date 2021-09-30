// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{handle_rpc_err, print_rpc_res_bytes, Config};
use forest_libp2p::{Multiaddr, Protocol};
use jsonrpc_v2::Error as JsonRpcError;
use rpc_client::{auth_new, DEFAULT_HOST};
use structopt::StructOpt;

use auth::*;

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

fn process_perms(perm: String) -> Result<Vec<String>, JsonRpcError> {
    match perm.as_str() {
        "admin" => Ok(ADMIN.to_owned()),
        "sign" => Ok(SIGN.to_owned()),
        "write" => Ok(WRITE.to_owned()),
        "read" => Ok(READ.to_owned()),
        _ => Err(JsonRpcError::INVALID_PARAMS),
    }
}

impl AuthCommands {
    pub async fn run(&self, cfg: Config) {
        match self {
            Self::CreateToken { perm } => {
                let perm: String = perm.parse().unwrap();
                let perms = process_perms(perm).map_err(handle_rpc_err).unwrap();
                print_rpc_res_bytes(auth_new((perms,)).await);
            }
            Self::ApiInfo { perm } => {
                let perm: String = perm.parse().unwrap();
                let perms = process_perms(perm).map_err(handle_rpc_err).unwrap();
                match auth_new((perms,)).await {
                    Ok(token) => {
                        let mut addr = Multiaddr::empty();
                        addr.push(Protocol::Ip4(DEFAULT_HOST.parse().unwrap()));
                        addr.push(Protocol::Tcp(cfg.rpc_port.parse().unwrap()));
                        addr.push(Protocol::Http);
                        println!(
                            "FULLNODE_API_INFO=\"{}:{}\"",
                            String::from_utf8(token)
                                .map_err(|e| handle_rpc_err(e.into()))
                                .unwrap(),
                            addr.to_string()
                        );
                    }
                    Err(e) => handle_rpc_err(e),
                };
            }
        }
    }
}
