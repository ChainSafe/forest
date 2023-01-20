// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Config;
use crate::cli::cli_error_and_die;
use forest_libp2p::{Multiaddr, Protocol};
use forest_rpc_api::data_types::AddrInfo;
use std::collections::HashSet;
use structopt::StructOpt;

use super::{handle_rpc_err, print_stdout};
use forest_rpc_client::net_ops::*;

#[derive(Debug, StructOpt)]
pub enum NetCommands {
    /// Lists `libp2p` swarm listener addresses
    Listen,
    /// Lists `libp2p` swarm peers
    Peers,
    /// Connects to a peer by its peer ID and multi-addresses
    Connect {
        /// Multi-address (with `/p2p/` protocol)
        address: String,
    },
    /// Disconnects from a peer by it's peer ID
    Disconnect {
        /// Peer ID to disconnect from
        id: String,
    },
}

impl NetCommands {
    pub async fn run(&self, config: Config) -> anyhow::Result<()> {
        match self {
            Self::Listen => {
                let info = net_addrs_listen((), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;
                let addresses: Vec<String> = info
                    .addrs
                    .iter()
                    .map(|addr| format!("{}/p2p/{}", addr, info.id))
                    .collect();
                print_stdout(addresses.join("\n"));
                Ok(())
            }
            Self::Peers => {
                let addrs = net_peers((), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;
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
                Ok(())
            }
            Self::Connect { address } => {
                let addr: Multiaddr = address
                    .parse()
                    .map_err(|e| {
                        cli_error_and_die(format!("Error parsing multiaddr. Error was: {e}"), 1);
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
                    cli_error_and_die("Needs a /p2p/ protocol present in multiaddr", 1)
                }

                let addrs = vec![addr];
                let addr_info = AddrInfo {
                    id: id.clone(),
                    addrs,
                };

                net_connect((addr_info,), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;
                println!("connect {id}: success");
                Ok(())
            }
            Self::Disconnect { id } => {
                net_disconnect((id.to_owned(),), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;
                println!("disconnect {id}: success");
                Ok(())
            }
        }
    }
}
