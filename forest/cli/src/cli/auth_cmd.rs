// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{handle_rpc_err, print_rpc_res_bytes, Config};
use forest_libp2p::{Multiaddr, Protocol};
use forest_rpc_api::auth_api::AuthNewParams;
use forest_rpc_client::auth_new;
use jsonrpc_v2::Error as JsonRpcError;
use structopt::StructOpt;

use forest_auth::*;

#[derive(Debug, StructOpt)]
pub enum AuthCommands {
    /// Create a new Authentication token with given permission
    CreateToken {
        /// permission to assign to the token, one of: read, write, sign, admin
        #[structopt(short, long)]
        perm: String,
    },
    /// Get RPC API Information
    ApiInfo {
        /// permission to assign the token, one of: read, write, sign, admin
        #[structopt(short, long)]
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
    pub async fn run(&self, config: Config) {
        match self {
            Self::CreateToken { perm } => {
                let perm: String = perm.parse().unwrap();
                let perms = process_perms(perm).map_err(handle_rpc_err).unwrap();
                let token_exp = config.client.token_exp;
                let auth_params = AuthNewParams { perms, token_exp };
                print_rpc_res_bytes(auth_new(auth_params, &config.client.rpc_token).await);
            }
            Self::ApiInfo { perm } => {
                let perm: String = perm.parse().unwrap();
                let perms = process_perms(perm).map_err(handle_rpc_err).unwrap();
                let token_exp = config.client.token_exp;
                let auth_params = AuthNewParams { perms, token_exp };
                match auth_new(auth_params, &config.client.rpc_token).await {
                    Ok(token) => {
                        let mut addr = Multiaddr::empty();
                        addr.push(config.client.rpc_address.ip().into());
                        addr.push(Protocol::Tcp(config.client.rpc_address.port()));
                        addr.push(Protocol::Http);
                        println!(
                            "FULLNODE_API_INFO=\"{}:{}\"",
                            String::from_utf8(token)
                                .map_err(|e| handle_rpc_err(e.into()))
                                .unwrap(),
                            addr
                        );
                    }
                    Err(e) => handle_rpc_err(e),
                };
            }
        }
    }
}
