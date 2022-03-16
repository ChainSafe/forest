// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use libp2p::Multiaddr;
use serde::Deserialize;

/// Libp2p config for the Forest node.
#[derive(Debug, Deserialize)]
pub struct Libp2pConfig {
    /// Local address.
    pub listening_multiaddr: Multiaddr,
    /// Bootstrap peer list.
    pub bootstrap_peers: Vec<Multiaddr>,
    /// Mdns discovery enabled.
    pub mdns: bool,
    /// Kademlia discovery enabled.
    pub kademlia: bool,
    /// Target peer count.
    pub target_peer_count: u32,
}

impl Libp2pConfig {
    pub fn new(bootstrap: &[String]) -> Self {
        let bootstrap_peers = bootstrap
            .iter()
            .map(|node| node.parse().unwrap())
            .collect();
        Self {
            listening_multiaddr: "/ip4/0.0.0.0/tcp/0".parse().unwrap(),
            bootstrap_peers,
            mdns: false,
            kademlia: true,
            target_peer_count: 75,
        }
    }
}
