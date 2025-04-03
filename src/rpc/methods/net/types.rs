// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::lotus_json::lotus_json_with_self;
use crate::utils::p2p::MultiaddrExt as _;
use libp2p::{Multiaddr, PeerId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// Net API
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct AddrInfo {
    #[serde(rename = "ID")]
    pub id: String,
    #[schemars(with = "ahash::HashSet<String>")]
    pub addrs: ahash::HashSet<Multiaddr>,
}
lotus_json_with_self!(AddrInfo);

impl AddrInfo {
    pub fn new(peer: PeerId, addrs: ahash::HashSet<Multiaddr>) -> Self {
        Self {
            id: peer.to_string(),
            addrs: addrs.into_iter().map(|addr| addr.without_p2p()).collect(),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, JsonSchema, PartialEq)]
pub struct NetInfoResult {
    pub num_peers: usize,
    pub num_connections: u32,
    pub num_pending: u32,
    pub num_pending_incoming: u32,
    pub num_pending_outgoing: u32,
    pub num_established: u32,
}
lotus_json_with_self!(NetInfoResult);

impl From<libp2p::swarm::NetworkInfo> for NetInfoResult {
    fn from(i: libp2p::swarm::NetworkInfo) -> Self {
        let counters = i.connection_counters();
        Self {
            num_peers: i.num_peers(),
            num_connections: counters.num_connections(),
            num_pending: counters.num_pending(),
            num_pending_incoming: counters.num_pending_incoming(),
            num_pending_outgoing: counters.num_pending_outgoing(),
            num_established: counters.num_established(),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, JsonSchema, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct NatStatusResult {
    pub reachability: i32,
    pub public_addrs: Option<Vec<String>>,
}
lotus_json_with_self!(NatStatusResult);

impl NatStatusResult {
    // See <https://github.com/libp2p/go-libp2p/blob/164adb40fef9c19774eb5fe6d92afb95c67ba83c/core/network/network.go#L93>
    pub fn reachability_as_str(&self) -> &'static str {
        match self.reachability {
            0 => "Unknown",
            1 => "Public",
            2 => "Private",
            _ => "(unrecognized)",
        }
    }
}

impl From<libp2p::autonat::NatStatus> for NatStatusResult {
    fn from(nat: libp2p::autonat::NatStatus) -> Self {
        use libp2p::autonat::NatStatus;

        // See <https://github.com/libp2p/go-libp2p/blob/91e1025f04519a5560361b09dfccd4b5239e36e6/core/network/network.go#L77>
        let (reachability, public_addrs) = match &nat {
            NatStatus::Unknown => (0, None),
            NatStatus::Public(addr) => (1, Some(vec![addr.to_string()])),
            NatStatus::Private => (2, None),
        };

        NatStatusResult {
            reachability,
            public_addrs,
        }
    }
}
