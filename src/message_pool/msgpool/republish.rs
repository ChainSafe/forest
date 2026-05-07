// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Tracks which CIDs were already broadcast in the current republish cycle
//! and exposes a trigger to wake the republish task early.

use ahash::HashSet;
use cid::Cid;
use parking_lot::RwLock as SyncRwLock;

use crate::message_pool::Error;

const REPUB_TRIGGER_CAPACITY: usize = 4;

pub(in crate::message_pool) struct RepublishState {
    republished: SyncRwLock<HashSet<Cid>>,
    trigger: flume::Sender<()>,
}

impl RepublishState {
    pub(in crate::message_pool) fn new() -> (Self, flume::Receiver<()>) {
        let (trigger, rx) = flume::bounded(REPUB_TRIGGER_CAPACITY);
        (
            Self {
                republished: SyncRwLock::default(),
                trigger,
            },
            rx,
        )
    }

    /// Returns `true` if the CID was newly inserted — callers use this to
    /// decide whether to wake the republish loop.
    pub(in crate::message_pool) fn mark_republished(&self, cid: Cid) -> bool {
        self.republished.write().insert(cid)
    }

    /// Wake the republish task early.
    pub(in crate::message_pool) async fn trigger(&self) -> Result<(), Error> {
        self.trigger
            .send_async(())
            .await
            .map_err(|e| Error::Other(format!("Republish receiver dropped: {e}")))
    }

    pub(in crate::message_pool) fn replace_with<I: IntoIterator<Item = Cid>>(&self, cids: I) {
        let mut set = self.republished.write();
        set.clear();
        set.extend(cids);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mark_republished_returns_true_only_on_first_insert() {
        let (state, _rx) = RepublishState::new();
        let cid = Cid::default();

        assert!(state.mark_republished(cid), "first insert should be new");
        assert!(
            !state.mark_republished(cid),
            "second insert should be a no-op",
        );
    }

    #[tokio::test]
    async fn trigger_succeeds_when_receiver_is_alive() {
        let (state, rx) = RepublishState::new();
        state.trigger().await.expect("send should succeed");
        rx.try_recv()
            .expect("trigger should be observable on the receiver");
    }

    #[test]
    fn replace_with_clears_then_inserts() {
        let (state, _rx) = RepublishState::new();
        let prior = Cid::default();
        state.mark_republished(prior);

        state.replace_with(std::iter::empty());
        assert!(
            state.mark_republished(prior),
            "set should be empty after clear-and-extend with empty iter",
        );

        state.replace_with([prior]);
        assert!(
            !state.mark_republished(prior),
            "prior CID should be present after replace_with",
        );
    }
}
