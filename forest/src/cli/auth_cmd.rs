// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{cli_error_and_die, handle_rpc_err, print_rpc_res};
use rpc_client::{auth_api_info, auth_new, auth_verify};
use structopt::StructOpt;

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
    #[structopt(about = "Get RPC API information")]
    ApiInfo {
        #[structopt(
            short,
            help = "permission to assign the token, one of: read, write, sign, admin"
        )]
        perm: String,
        #[structopt(
            short,
            help = "the admin token to use to create the multiaddress and auth header"
        )]
        admin_token: String,
    },
}

impl AuthCommands {
    pub async fn run(&self) {
        match self {
            Self::CreateToken { perm } => {
                let perm: String = perm.parse().unwrap();
                print_rpc_res(auth_new(perm).await);
            }
            Self::ApiInfo { perm, admin_token } => {
                let perm: String = perm.parse().unwrap();

                let verify_response = match auth_verify(admin_token.to_string()).await {
                    Ok(value) => value,
                    Err(error) => return handle_rpc_err(error),
                };

                if !verify_response {
                    cli_error_and_die("Error validating token", 1);
                }

                let response = auth_api_info(perm).await;
                print_rpc_res(response);
            }
        }
    }
}
