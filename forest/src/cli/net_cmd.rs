// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_libp2p::{Multiaddr, Multihash, Protocol};
use rpc_api::data_types::AddrInfo;
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
    /// Connects to a peer
    #[structopt(about = "Connect to a peer by its peer ID and multiaddresses")]
    Connect {
        #[structopt(short, about = "Peer ID to connect to")]
        id: String,
        #[structopt(short, about = "Multiaddresses (can be supplied multiple times)")]
        addresses: Vec<String>,
    },
    /// Disconnects from a peer
    #[structopt(about = "Disconnect from a peer by its peer ID")]
    Disconnect {
        #[structopt(short, about = "Peer ID to disconnect from")]
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
            Self::Connect { id, addresses } => {
                let (_base, data) =
                    multibase::decode(id).expect("decode provided multibase string");
                let peer_id = Multihash::from_bytes(&data)
                    .expect("parse multihash from decoded multibase bytes");

                let addrs = addresses
                    .iter()
                    .map(|addr| {
                        let mut address: Multiaddr =
                            addr.parse().expect("parse provided multiaddr from string");
                        address.push(Protocol::P2p(peer_id));
                        address
                    })
                    .collect();

                let addr_info = AddrInfo {
                    id: id.to_owned(),
                    addrs,
                };

                match net_connect((addr_info,)).await {
                    Ok(_) => {}
                    Err(e) => handle_rpc_err(e.into()),
                }
            }
            Self::Disconnect { id } => match net_disconnect((id.to_owned(),)).await {
                Ok(_) => {
                    todo!();
                }
                Err(e) => handle_rpc_err(e.into()),
            },
        }
    }
}
