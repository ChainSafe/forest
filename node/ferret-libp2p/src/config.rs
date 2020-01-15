// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use libp2p::gossipsub::Topic;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Libp2pConfig {
    pub listening_multiaddr: String,
    pub bootstrap_peers: Vec<String>,

    #[serde(skip_deserializing)] // Always use default
    pub pubsub_topics: Vec<Topic>,
}

impl Default for Libp2pConfig {
    fn default() -> Self {
        Libp2pConfig {
            listening_multiaddr: "/ip4/0.0.0.0/tcp/0".to_owned(),
            pubsub_topics: vec![
                Topic::new("/fil/blocks".to_owned()),
                Topic::new("/fil/messages".to_owned()),
            ],
            bootstrap_peers: vec![],
        }
    }
}
