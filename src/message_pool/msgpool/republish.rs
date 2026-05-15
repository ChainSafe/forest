// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Tracks which CIDs were already broadcast in the current republish cycle
//! and exposes a trigger to wake the republish task early.

use ahash::HashSet;
use cid::Cid;
use parking_lot::RwLock as SyncRwLock;

use crate::message_pool::Error;

const REPUB_TRIGGER_CAPACITY: usize = 1;

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

    /// Returns `true` if `cid` was seen by the republished state.
    pub(in crate::message_pool) fn was_republished(&self, cid: &Cid) -> bool {
        self.republished.read().contains(cid)
    }

    /// Wake the republish task early.
    pub(in crate::message_pool) fn trigger(&self) -> Result<(), Error> {
        match self.trigger.try_send(()) {
            Ok(()) | Err(flume::TrySendError::Full(_)) => Ok(()),
            Err(flume::TrySendError::Disconnected(_)) => {
                Err(Error::Other("republish receiver dropped".into()))
            }
        }
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
    fn was_republished_reflects_replace_with() {
        let (state, _rx) = RepublishState::new();
        let cid = Cid::default();

        assert!(
            !state.was_republished(&cid),
            "fresh state should not contain any CIDs",
        );

        state.replace_with([cid]);
        assert!(
            state.was_republished(&cid),
            "replace_with should populate the set",
        );

        state.replace_with(std::iter::empty());
        assert!(
            !state.was_republished(&cid),
            "replace_with with empty iter should clear the set",
        );
    }

    #[test]
    fn trigger_succeeds_when_receiver_is_alive() {
        let (state, rx) = RepublishState::new();
        state.trigger().expect("send should succeed");
        rx.try_recv()
            .expect("trigger should be observable on the receiver");
    }

    #[test]
    fn trigger_drops_silently_when_buffer_full() {
        let (state, _rx) = RepublishState::new();
        state.trigger().expect("first trigger should send");
        // Buffer (capacity 1) is now full; a second trigger must coalesce
        // silently instead of failing head_change.
        state
            .trigger()
            .expect("overflow trigger should be dropped silently");
    }

    #[test]
    fn trigger_errors_when_receiver_disconnected() {
        let (state, rx) = RepublishState::new();
        drop(rx);
        let err = state
            .trigger()
            .expect_err("disconnected receiver should surface as an error");
        assert!(matches!(err, Error::Other(_)));
    }
}
