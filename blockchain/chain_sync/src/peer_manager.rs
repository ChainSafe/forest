// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::sync::RwLock;
use blocks::Tipset;
use libp2p::core::PeerId;
use log::debug;
use rand::seq::SliceRandom;
use std::collections::HashMap;
use std::sync::Arc;

/// Thread safe peer manager which handles peer management for the `BlockSync` protocol.
#[derive(Default)]
pub struct PeerManager {
    // TODO potentially separate or expand to handle blocksync peers/ peers that haven't sent hello
    /// Hash set of full peers available
    full_peers: RwLock<HashMap<PeerId, Option<Arc<Tipset>>>>,
}

impl PeerManager {
    /// Adds a PeerId to the set of managed peers
    pub async fn add_peer(&self, peer_id: PeerId, ts: Option<Arc<Tipset>>) {
        debug!("Added PeerId to full peers list: {}", &peer_id);
        self.full_peers.write().await.insert(peer_id, ts);
    }

    /// Returns true if peer set is empty
    pub async fn is_empty(&self) -> bool {
        self.full_peers.read().await.is_empty()
    }

    /// Retrieves a cloned PeerId to be used to send network request
    pub async fn get_peer(&self) -> Option<PeerId> {
        // TODO this should prioritize peers with greater success rate and take a random sample
        // of the top `x` peers
        let peer_vec: Vec<PeerId> = self.full_peers.read().await.keys().cloned().collect();
        peer_vec.choose(&mut rand::thread_rng()).cloned()
    }

    /// Retrieves all tipsets from current peer set
    pub async fn get_peer_heads(&self) -> Vec<Arc<Tipset>> {
        self.full_peers
            .read()
            .await
            .iter()
            .filter_map(|(_, v)| v.clone())
            .collect()
    }

    /// Removes a peer from the set and returns true if the value was present previously
    pub async fn remove_peer(&self, peer_id: &PeerId) -> bool {
        self.full_peers.write().await.remove(peer_id).is_some()
    }

    /// Gets count of full peers managed
    pub async fn len(&self) -> usize {
        self.full_peers.read().await.len()
    }
}
