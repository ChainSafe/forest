// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::bucket::SyncBucketSet;
use blocks::Tipset;
use libp2p::core::PeerId;
use std::sync::Arc;

/// Manages tipsets pulled from network to be synced
#[derive(Default)]
pub struct SyncManager {
    sync_queue: SyncBucketSet,
}

impl SyncManager {
    /// Schedules a new tipset to be handled by the sync manager
    pub fn schedule_tipset(&mut self, tipset: Arc<Tipset>) {
        // TODO implement interactions for syncing state when SyncManager built out
        self.sync_queue.insert(tipset);
    }
    /// Retrieves the heaviest tipset in the sync queue
    pub fn select_sync_target(&self) -> Option<Arc<Tipset>> {
        self.sync_queue.heaviest()
    }
    /// Sets the PeerId indicating the head tipset
    pub fn set_peer_head(&self, _peer: &PeerId, _ts: Tipset) {
        // TODO
        todo!()
    }
}
