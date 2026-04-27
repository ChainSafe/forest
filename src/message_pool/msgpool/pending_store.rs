// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Pending message storage.
//!
//! [`PendingStore`] owns the per-actor [`MsgSet`] map and the
//! [`MpoolUpdate`] broadcast channel. It is the single place in the code
//! where the pending map is mutated, which lets observers subscribe to one
//! event stream.

use std::sync::Arc;

use ahash::{HashMap, HashMapExt};
use parking_lot::RwLock as SyncRwLock;
use tokio::sync::broadcast;

use crate::message::SignedMessage;
use crate::message_pool::errors::Error;
use crate::message_pool::msgpool::events::{MPOOL_UPDATE_CHANNEL_CAPACITY, MpoolUpdate};
use crate::message_pool::msgpool::msg_set::{MsgSet, MsgSetLimits, StrictnessPolicy};
use crate::message_pool::msgpool::msg_pool::TrustPolicy;
use crate::shim::address::Address;

/// A shared, event-emitting pending-message store.
#[derive(Clone)]
pub(in crate::message_pool) struct PendingStore {
    inner: Arc<Inner>,
}

struct Inner {
    /// Per-resolved-address pending messages.
    pending: SyncRwLock<HashMap<Address, MsgSet>>,
    /// Broadcast channel for [`MpoolUpdate`] events.
    events: broadcast::Sender<MpoolUpdate>,
    /// Per-actor pending-message caps captured once from the provider.
    limits: MsgSetLimits,
}

impl PendingStore {
    /// Construct an empty store with the given per-actor limits.
    pub(in crate::message_pool) fn new(limits: MsgSetLimits) -> Self {
        let (events, _) = broadcast::channel(MPOOL_UPDATE_CHANNEL_CAPACITY);
        Self {
            inner: Arc::new(Inner {
                pending: SyncRwLock::new(HashMap::new()),
                events,
                limits,
            }),
        }
    }

    /// Insert a signed message under its already-resolved sender address.
    ///
    /// On success, emits a single [`MpoolUpdate::Add`] carrying the inserted
    /// message.
    pub(in crate::message_pool) fn insert(
        &self,
        resolved_from: Address,
        msg: SignedMessage,
        state_sequence: u64,
        trust: TrustPolicy,
        strictness: StrictnessPolicy,
    ) -> Result<(), Error> {
        let event_msg = self.has_subscribers().then(|| msg.clone());

        {
            let mut pending = self.inner.pending.write();
            let mset = pending
                .entry(resolved_from)
                .or_insert_with(|| MsgSet::new(state_sequence));
            mset.add(msg, strictness, trust, self.inner.limits)?;
        }

        if let Some(m) = event_msg {
            // send() only fails when there are zero receivers; a race with
            // the last receiver dropping is benign and intentionally ignored.
            let _ = self.inner.events.send(MpoolUpdate::Add(m));
        }
        Ok(())
    }

    /// Remove the message at `sequence` for `from` (which must already be in
    /// resolved-key form).
    /// Returns the removed message if one was present. Emits a single
    /// [`MpoolUpdate::Remove`] per actual removal
    pub(in crate::message_pool) fn remove(
        &self,
        from: &Address,
        sequence: u64,
        applied: bool,
    ) -> Option<SignedMessage> {
        let removed = {
            let mut pending = self.inner.pending.write();
            let mset = pending.get_mut(from)?;
            let removed = mset.rm(sequence, applied);
            if mset.msgs.is_empty() {
                pending.remove(from);
            }
            removed
        };

        if let Some(msg) = &removed
            && self.has_subscribers()
        {
            let _ = self.inner.events.send(MpoolUpdate::Remove(msg.clone()));
        }
        removed
    }

    /// Deep-clone of the pending map — one read-lock acquisition.
    pub(in crate::message_pool) fn snapshot(&self) -> HashMap<Address, MsgSet> {
        self.inner.pending.read().clone()
    }

    /// Deep-clone the [`MsgSet`] for a single sender, if present.
    pub(in crate::message_pool) fn snapshot_for(&self, addr: &Address) -> Option<MsgSet> {
        self.inner.pending.read().get(addr).cloned()
    }

    /// Subscribe to the [`MpoolUpdate`] stream. Returned receiver is
    /// independent; dropping it does not affect other subscribers.
    #[allow(dead_code)] // consumed by MessagePool::subscribe_to_updates / external subscribers.
    pub fn subscribe(&self) -> broadcast::Receiver<MpoolUpdate> {
        self.inner.events.subscribe()
    }

    /// `true` while at least one subscriber holds a live.
    pub(in crate::message_pool) fn has_subscribers(&self) -> bool {
        self.inner.events.receiver_count() > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::MessageRead as _;
    use crate::shim::econ::TokenAmount;
    use crate::shim::message::Message as ShimMessage;
    use tokio::sync::broadcast::error::TryRecvError;

    /// Default limits used by PendingStore unit tests. Picked high enough
    /// that nonce/gap behaviour, not capacity, drives the outcomes.
    const TEST_LIMITS: MsgSetLimits = MsgSetLimits {
        trusted: 1000,
        untrusted: 1000,
    };

    fn make_smsg(from: Address, seq: u64, premium: u64) -> SignedMessage {
        SignedMessage::mock_bls_signed_message(ShimMessage {
            from,
            sequence: seq,
            gas_premium: TokenAmount::from_atto(premium),
            gas_limit: 1_000_000,
            ..ShimMessage::default()
        })
    }

    fn assert_add(update: MpoolUpdate, expected_seq: u64) {
        match update {
            MpoolUpdate::Add(m) => assert_eq!(m.sequence(), expected_seq),
            other => panic!("expected Add, got {other:?}"),
        }
    }

    fn assert_remove(update: MpoolUpdate, expected_seq: u64) {
        match update {
            MpoolUpdate::Remove(m) => assert_eq!(m.sequence(), expected_seq),
            other => panic!("expected Remove, got {other:?}"),
        }
    }

    #[test]
    fn insert_emits_add_and_stores_message() {
        let store = PendingStore::new(TEST_LIMITS);
        let mut rx = store.subscribe();
        let addr = Address::new_id(1);

        store
            .insert(
                addr,
                make_smsg(addr, 0, 100),
                0,
                TrustPolicy::Trusted,
                StrictnessPolicy::Relaxed,
            )
            .unwrap();

        assert_add(rx.try_recv().unwrap(), 0);
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)), "expected empty channel");
        assert_eq!(store.snapshot_for(&addr).unwrap().next_sequence, 1);
    }

    #[test]
    fn rbf_replacement_emits_add_for_the_new_message() {
        let store = PendingStore::new(TEST_LIMITS);
        let mut rx = store.subscribe();
        let addr = Address::new_id(1);

        store
            .insert(
                addr,
                make_smsg(addr, 0, 100),
                0,
                TrustPolicy::Trusted,
                StrictnessPolicy::Relaxed,
            )
            .unwrap();
        store
            .insert(
                addr,
                make_smsg(addr, 0, 200), // higher premium → RBF
                0,
                TrustPolicy::Trusted,
                StrictnessPolicy::Relaxed,
            )
            .unwrap();

        assert_add(rx.try_recv().unwrap(), 0);
        assert_add(rx.try_recv().unwrap(), 0);
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)), "expected empty channel");
    }

    #[test]
    fn remove_emits_remove_once_then_is_idempotent() {
        let store = PendingStore::new(TEST_LIMITS);
        let mut rx = store.subscribe();
        let addr = Address::new_id(1);

        store
            .insert(
                addr,
                make_smsg(addr, 0, 100),
                0,
                TrustPolicy::Trusted,
                StrictnessPolicy::Relaxed,
            )
            .unwrap();
        let _add = rx.try_recv().unwrap();

        assert!(store.remove(&addr, 0, true).is_some());
        assert_remove(rx.try_recv().unwrap(), 0);

        // Second remove is a no-op — sender is already gone.
        assert!(store.remove(&addr, 0, true).is_none());
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)), "expected empty channel");
    }

    #[test]
    fn remove_of_unknown_sender_is_silent() {
        let store = PendingStore::new(TEST_LIMITS);
        let mut rx = store.subscribe();
        let addr = Address::new_id(42);

        assert!(store.remove(&addr, 0, true).is_none());
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)), "expected empty channel");
    }

    #[test]
    fn insert_without_subscribers_skips_message_clone() {
        // Regression guard for the has_subscribers fast-path: insert must
        // succeed and the store must reflect the message even when the emit
        // branch is elided entirely.
        let store = PendingStore::new(TEST_LIMITS);
        let addr = Address::new_id(1);

        assert!(!store.has_subscribers());
        store
            .insert(
                addr,
                make_smsg(addr, 0, 100),
                0,
                TrustPolicy::Trusted,
                StrictnessPolicy::Relaxed,
            )
            .unwrap();
        assert_eq!(store.snapshot_for(&addr).unwrap().next_sequence, 1);
    }

    #[test]
    fn snapshot_is_a_deep_copy() {
        let store = PendingStore::new(TEST_LIMITS);
        let addr = Address::new_id(1);
        store
            .insert(
                addr,
                make_smsg(addr, 0, 100),
                0,
                TrustPolicy::Trusted,
                StrictnessPolicy::Relaxed,
            )
            .unwrap();

        let mut snap = store.snapshot();
        snap.clear();
        assert!(
            !store.snapshot().is_empty(),
            "mutating the snapshot must not affect the store"
        );
    }

    #[test]
    fn clone_is_cheap_and_shares_state() {
        // The handle pattern: cloning the store gives another view over the
        // same pending map and the same broadcast channel.
        let store = PendingStore::new(TEST_LIMITS);
        let handle = store.clone();
        let mut rx = handle.subscribe();
        let addr = Address::new_id(7);

        store
            .insert(
                addr,
                make_smsg(addr, 0, 100),
                0,
                TrustPolicy::Trusted,
                StrictnessPolicy::Relaxed,
            )
            .unwrap();

        assert_add(rx.try_recv().unwrap(), 0);
        assert_eq!(handle.snapshot_for(&addr).unwrap().next_sequence, 1);
    }

    #[test]
    fn remove_clears_empty_sender_bucket() {
        let store = PendingStore::new(TEST_LIMITS);
        let addr = Address::new_id(1);
        store
            .insert(
                addr,
                make_smsg(addr, 0, 100),
                0,
                TrustPolicy::Trusted,
                StrictnessPolicy::Relaxed,
            )
            .unwrap();
        store.remove(&addr, 0, true);
        assert!(
            store.snapshot().is_empty(),
            "removing the last message for an actor should drop the bucket"
        );
    }
}
