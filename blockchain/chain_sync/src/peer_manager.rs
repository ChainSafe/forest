// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use libp2p::core::PeerId;
use log::debug;
use parking_lot::RwLock;
use std::collections::HashSet;

/// Thread safe peer manager
#[derive(Default)]
pub struct PeerManager {
    /// Hash set of full peers available
    full_peers: RwLock<HashSet<PeerId>>,
}

impl PeerManager {
    /// Adds a PeerId to the set of managed peers
    pub fn _add_peer(&self, peer_id: PeerId) {
        debug!("Added PeerId to full peers list: {}", &peer_id);
        self.full_peers.write().insert(peer_id);
    }

    /// Returns true if peer set is empty
    pub fn is_empty(&self) -> bool {
        self.full_peers.read().is_empty()
    }

    /// Retrieves a cloned PeerId to be used to send network request
    pub fn get_peer(&self) -> Option<PeerId> {
        // TODO replace this with a shuffled or more random sample
        self.full_peers.read().iter().next().cloned()
    }
}
