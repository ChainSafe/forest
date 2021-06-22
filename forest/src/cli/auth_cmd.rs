// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use jsonrpc_v2::Error as JsonRpcError;
use structopt::StructOpt;

use super::{handle_rpc_err, print_rpc_res_bytes};
use auth::*;
use rpc_client::auth_ops::*;

#[derive(Debug, StructOpt)]
pub enum AuthCommands {
    /// Create a new Authentication token with given permission
    #[structopt(about = "<String> Create Authentication token with given permission")]
    CreateToken {
        #[structopt(
            short,
            help = "permission to assign to the token, one of: read, write, sign, admin"
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
    pub async fn run(&self) {
        match self {
            Self::CreateToken { perm } => {
                let perm: String = perm.parse().unwrap();
                let perms = process_perms(perm).map_err(handle_rpc_err).unwrap();
                print_rpc_res_bytes(auth_new((perms,)).await);
            }
        }
    }
}
