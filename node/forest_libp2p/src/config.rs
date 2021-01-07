// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use libp2p::Multiaddr;
use networks::DEFAULT_BOOTSTRAP;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Libp2pConfig {
    pub listening_multiaddr: Multiaddr,
    pub bootstrap_peers: Vec<Multiaddr>,
    pub mdns: bool,
    pub kademlia: bool,
}

impl Default for Libp2pConfig {
    fn default() -> Self {
        let bootstrap_peers = DEFAULT_BOOTSTRAP
            .iter()
            .map(|node| node.parse().unwrap())
            .collect();
        Self {
            listening_multiaddr: "/ip4/0.0.0.0/tcp/0".parse().unwrap(),
            bootstrap_peers,
            mdns: true,
            kademlia: true,
        }
    }
}
