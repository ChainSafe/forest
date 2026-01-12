// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::libp2p::{Multiaddr, Protocol};
use crate::rpc::{self, net::AddrInfo, prelude::*};
use ahash::{HashMap, HashSet};
use cid::multibase;
use clap::Subcommand;
use itertools::Itertools;

use crate::cli::subcommands::cli_error_and_die;

#[derive(Debug, Subcommand)]
pub enum NetCommands {
    /// Lists `libp2p` swarm listener addresses
    Listen,
    /// Lists `libp2p` swarm network info
    Info,
    /// Lists `libp2p` swarm peers
    Peers {
        /// Print agent name
        #[arg(short, long)]
        agent: bool,
    },
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
    /// Print information about reachability from the internet
    Reachability,
}

impl NetCommands {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::Listen => {
                let info = NetAddrsListen::call(&client, ()).await?;
                let addresses: Vec<String> = info
                    .addrs
                    .iter()
                    .map(|addr| format!("{}/p2p/{}", addr, info.id))
                    .collect();
                println!("{}", addresses.join("\n"));
                Ok(())
            }
            Self::Info => {
                let info = NetInfo::call(&client, ()).await?;
                println!("forest libp2p swarm info:");
                println!("num peers: {}", info.num_peers);
                println!("num connections: {}", info.num_connections);
                println!("num pending: {}", info.num_pending);
                println!("num pending incoming: {}", info.num_pending_incoming);
                println!("num pending outgoing: {}", info.num_pending_outgoing);
                println!("num established: {}", info.num_established);
                Ok(())
            }
            Self::Peers { agent } => {
                let addrs = NetPeers::call(&client, ()).await?;
                let peer_to_agents: HashMap<String, String> = if agent {
                    let agents = futures::future::join_all(
                        addrs
                            .iter()
                            .map(|info| NetAgentVersion::call(&client, (info.id.clone(),))),
                    )
                    .await
                    .into_iter()
                    .map(|res| res.unwrap_or_else(|_| "<agent unknown>".to_owned()));

                    HashMap::from_iter(
                        addrs
                            .iter()
                            .map(|info| info.id.to_owned())
                            .zip(agents.into_iter()),
                    )
                } else {
                    HashMap::default()
                };

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
                            .unique()
                            .collect();
                        if addresses.is_empty() {
                            return None;
                        }

                        let result = format!("{}, [{}]", info.id, addresses.join(", "));

                        if agent {
                            Some(
                                [
                                    result,
                                    peer_to_agents
                                        .get(&info.id)
                                        .cloned()
                                        .unwrap_or_else(|| "<agent unknown>".to_owned()),
                                ]
                                .join(", "),
                            )
                        } else {
                            Some(result)
                        }
                    })
                    .collect();
                println!("{}", output.join("\n"));
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

                let addrs = HashSet::from_iter([addr]);
                let addr_info = AddrInfo {
                    id: id.clone(),
                    addrs,
                };

                NetConnect::call(&client, (addr_info,)).await?;
                println!("connect {id}: success");
                Ok(())
            }
            Self::Disconnect { id } => {
                NetDisconnect::call(&client, (id.clone(),)).await?;
                println!("disconnect {id}: success");
                Ok(())
            }
            Self::Reachability => {
                let nat_status = NetAutoNatStatus::call(&client, ()).await?;
                println!("AutoNAT status:  {}", nat_status.reachability_as_str());
                if let Some(public_addrs) = nat_status.public_addrs
                    && !public_addrs.is_empty()
                {
                    // Format is compatible with Go code:
                    // `fmt.Println("Public address:", []string{"foo", "bar"})`
                    println!("Public address: [{}]", public_addrs.join(" "));
                }
                Ok(())
            }
        }
    }
}
