// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use super::bucket::SyncBucketSet;
use blocks::Tipset;
use libp2p::core::PeerId;

#[derive(Default)]
pub struct SyncManager<'a> {
    sync_queue: SyncBucketSet<'a>,
}

impl<'a> SyncManager<'a> {
    pub fn schedule_tipset(&mut self, tipset: &'a Tipset) {
        // TODO implement interactions for syncing state when SyncManager built out
        self.sync_queue.insert(tipset);
    }
    pub fn select_sync_target(&self) -> Option<&'a Tipset> {
        self.sync_queue.heaviest()
    }
    pub fn set_peer_head(&self, _peer: PeerId, _ts: Tipset) {
        // TODO
        todo!()
    }
}
