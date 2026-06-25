// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Event types published by the pending pool.

use crate::message::SignedMessage;
use tokio::sync::broadcast;

pub(in crate::message_pool) const MPOOL_UPDATE_CHANNEL_CAPACITY: usize = 256;

/// A change to the pending pool.
#[derive(Clone, Debug)]
pub enum MpoolUpdate {
    Add(SignedMessage),
    #[allow(dead_code)]
    Remove(SignedMessage),
}

/// Subscribe-only handle to the pending pool's [`MpoolUpdate`] broadcast bus.
///
/// Hands out independent receivers via [`subscribe`](Self::subscribe), each with
/// its own cursor. The inner `Sender` is private and never leaves the message
/// pool, so holders can listen but cannot publish events — the send capability
/// stays with [`PendingStore`](super::pending_store::PendingStore).
#[derive(Clone, Debug)]
pub struct MpoolSubscriber(broadcast::Sender<MpoolUpdate>);

impl MpoolSubscriber {
    pub(in crate::message_pool) fn new(events: broadcast::Sender<MpoolUpdate>) -> Self {
        Self(events)
    }

    /// A detached handle with no producer behind it: its receivers never observe
    /// any event. Used by standalone contexts (tests, snapshot tools, the
    /// offline server) that have no live mempool attached.
    pub fn dummy() -> Self {
        Self(broadcast::channel(1).0)
    }

    /// Open a fresh receiver that observes every [`MpoolUpdate`] published from
    /// this point on.
    pub(crate) fn subscribe(&self) -> broadcast::Receiver<MpoolUpdate> {
        self.0.subscribe()
    }
}
