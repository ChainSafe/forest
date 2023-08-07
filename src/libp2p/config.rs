// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use libp2p::Multiaddr;
use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::net::Ipv4Addr;

/// Libp2p configuration for the Forest node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub struct Libp2pConfig {
    /// Local addresses. TCP and WebSocket with DNS are supported. By making it
    /// empty, the libp2p node will not be capable of working as a callee but
    /// can still work as a dialer
    #[cfg_attr(test, arbitrary(gen(
        |g: &mut quickcheck::Gen| {
            let addr = Ipv4Addr::arbitrary(&mut *g);
            let n = u8::arbitrary(g) as usize;
            vec![addr.into(); n]
        }
    )))]
    pub listening_multiaddrs: Vec<Multiaddr>,
    /// Bootstrap peer list.
    #[cfg_attr(test, arbitrary(gen(
        |g| vec![Ipv4Addr::arbitrary(g).into()]
    )))]
    pub bootstrap_peers: Vec<Multiaddr>,
    /// MDNS discovery enabled.
    pub mdns: bool,
    /// Kademlia discovery enabled.
    pub kademlia: bool,
    /// Target peer count.
    pub target_peer_count: u32,
}

impl Default for Libp2pConfig {
    fn default() -> Self {
        Self {
            listening_multiaddrs: vec!["/ip4/0.0.0.0/tcp/0".parse().expect("Infallible")],
            bootstrap_peers: vec![],
            mdns: false,
            kademlia: true,
            target_peer_count: 75,
        }
    }
}
