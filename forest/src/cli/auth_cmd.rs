// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::print_rpc_res;
use rpc_client::auth_new;
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
}

impl AuthCommands {
    pub async fn run(&self) {
        match self {
            Self::CreateToken { perm } => {
                let perm: String = perm.parse().unwrap();
                print_rpc_res(auth_new(perm).await);
            }
        }
    }
}
