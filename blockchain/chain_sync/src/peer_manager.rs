// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::sync::RwLock;
use libp2p::core::PeerId;
use log::debug;
use std::collections::HashSet;

/// Thread safe peer manager
#[derive(Default)]
pub struct PeerManager {
    /// Hash set of full peers available
    full_peers: RwLock<HashSet<PeerId>>,
}

impl PeerManager {
    /// Adds a PeerId to the set of managed peers
    pub async fn add_peer(&self, peer_id: PeerId) {
        debug!("Added PeerId to full peers list: {}", &peer_id);
        self.full_peers.write().await.insert(peer_id);
    }

    /// Returns true if peer set is empty
    pub async fn is_empty(&self) -> bool {
        self.full_peers.read().await.is_empty()
    }

    /// Retrieves a cloned PeerId to be used to send network request
    pub async fn get_peer(&self) -> Option<PeerId> {
        // TODO replace this with a shuffled or more random sample
        self.full_peers.read().await.iter().next().cloned()
    }

    /// Removes a peer from the set and returns true if the value was present previously
    pub async fn remove_peer(&self, peer_id: &PeerId) -> bool {
        // TODO replace this with a shuffled or more random sample
        self.full_peers.write().await.remove(peer_id)
    }

    /// Gets count of full peers managed
    pub async fn len(&self) -> usize {
        self.full_peers.read().await.len()
    }
}
