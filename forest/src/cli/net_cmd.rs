// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use structopt::StructOpt;

use super::{handle_rpc_err, print_stdout};
use rpc_client::net_ops::*;

#[derive(Debug, StructOpt)]
pub enum NetCommands {
    /// Lists libp2p swarm listener addresses
    #[structopt(about = "List listen addresses")]
    Listen,
    /// Lists libp2p swarm peers
    #[structopt(about = "Print peers")]
    Peers,
    // TODO: connect, disconnect
}

impl NetCommands {
    pub async fn run(&self) {
        match self {
            Self::Listen => match net_addrs_listen(()).await {
                Ok(info) => {
                    let addresses: Vec<String> = info
                        .addrs
                        .iter()
                        .map(|addr| format!("{}/p2p/{}", addr.to_string(), info.id.to_string()))
                        .collect();

                    print_stdout(addresses.join("\n"));
                }
                Err(e) => handle_rpc_err(e.into()),
            },
            Self::Peers => match net_peers(()).await {
                Ok(addrs) => {
                    let output: Vec<String> = addrs
                        .iter()
                        .map(|info| {
                            let addresses: Vec<String> =
                                info.addrs.iter().map(|addr| addr.to_string()).collect();

                            format!("{}, [{}]", info.id, addresses.join(", "))
                        })
                        .collect();

                    print_stdout(output.join("\n"));
                }
                Err(e) => handle_rpc_err(e.into()),
            },
        } // TODO: connect, disconnect
    }
}
