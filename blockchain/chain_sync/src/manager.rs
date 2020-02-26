// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::bucket::SyncBucketSet;
use blocks::Tipset;
use libp2p::core::PeerId;
use std::sync::Arc;

#[derive(Default)]
pub struct SyncManager {
    sync_queue: SyncBucketSet,
}

impl SyncManager {
    pub fn schedule_tipset(&mut self, tipset: Arc<Tipset>) {
        // TODO implement interactions for syncing state when SyncManager built out
        self.sync_queue.insert(tipset);
    }
    pub fn select_sync_target(&self) -> Option<Arc<Tipset>> {
        self.sync_queue.heaviest()
    }
    pub fn set_peer_head(&self, _peer: PeerId, _ts: Tipset) {
        // TODO
        todo!()
    }
}
