// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_libp2p::{Multiaddr, Protocol};
use rpc_api::data_types::AddrInfo;
use std::collections::HashSet;
use structopt::StructOpt;

use crate::cli::cli_error_and_die;

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
    /// Connects to a peer
    #[structopt(about = "Connect to a peer by its peer ID and multiaddresses")]
    Connect {
        #[structopt(about = "Multiaddr (with /p2p/ protocol)")]
        address: String,
    },
    /// Disconnects from a peer
    #[structopt(about = "Disconnect from a peer by its peer ID")]
    Disconnect {
        #[structopt(about = "Peer ID to disconnect from")]
        id: String,
    },
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
                Err(e) => handle_rpc_err(e),
            },
            Self::Peers => match net_peers(()).await {
                Ok(addrs) => {
                    let output: Vec<String> = addrs
                        .into_iter()
                        .filter_map(|info| {
                            let addresses: Vec<String> = info
                                .addrs
                                .into_iter()
                                .filter(|addr| match addr.iter().next().unwrap() {
                                    Protocol::Ip4(ip_addr) => !ip_addr.is_loopback(),
                                    Protocol::Ip6(ip_addr) => !ip_addr.is_loopback(),
                                    _ => true,
                                })
                                .map(|addr| addr.to_string())
                                .collect::<HashSet<_>>()
                                .into_iter()
                                .collect();

                            if addresses.is_empty() {
                                return None;
                            }

                            Some(format!("{}, [{}]", info.id, addresses.join(", ")))
                        })
                        .collect();

                    print_stdout(output.join("\n"));
                }
                Err(e) => handle_rpc_err(e),
            },
            Self::Connect { address } => {
                let addr: Multiaddr = address
                    .parse()
                    .map_err(|e| {
                        cli_error_and_die(&format!("Error parsing multiaddr. Error was: {}", e), 1);
                    })
                    .expect("Parse provided multiaddr from string");

                let mut id = "".to_owned();

                for protocol in addr.iter() {
                    if let Protocol::P2p(p2p) = protocol {
                        id = multibase::encode(multibase::Base::Base58Btc, p2p.to_bytes());
                        id = id.split_off(1);
                    }
                }

                if id.is_empty() {
                    cli_error_and_die("Needs a /p2p/ protocol present in multiaddr", 1);
                    return;
                }

                let addrs = vec![addr];
                let addr_info = AddrInfo {
                    id: id.clone(),
                    addrs,
                };

                match net_connect((addr_info,)).await {
                    Ok(_) => {
                        println!("connect {}: success", id);
                    }
                    Err(e) => handle_rpc_err(e),
                }
            }
            Self::Disconnect { id } => match net_disconnect((id.to_owned(),)).await {
                Ok(_) => {
                    todo!();
                }
                Err(e) => handle_rpc_err(e),
            },
        }
    }
}
