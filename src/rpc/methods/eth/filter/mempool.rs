// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::prelude::*;
use crate::eth::EthChainId as EthChainIdType;
use crate::message::SignedMessage;
use crate::message_pool::MpoolUpdate;
use crate::rpc::Arc;
use crate::rpc::eth::eth_tx_hash_from_signed_message;
use crate::rpc::eth::types::EthHash;
use crate::rpc::eth::{FilterID, filter::Filter, filter::FilterManager};
use crate::shim::fvm_shared_latest::clock::ChainEpoch;
use ahash::AHashMap as HashMap;
use anyhow::{Context, Result};
use indexmap::IndexSet;
use parking_lot::{Mutex, RwLock};
use std::any::Any;
use tokio::sync::broadcast;

/// Factory that yields a fresh independent `broadcast::Receiver<MpoolUpdate>`
/// on each call. Wraps the `MessagePool` so the filter layer never sees the
/// pool's broadcast `Sender` directly — preserves the send-only encapsulation
/// owned by the message pool module.
#[derive(Clone)]
pub struct MpoolSubscriber {
    inner: Arc<dyn Fn() -> broadcast::Receiver<MpoolUpdate> + Send + Sync>,
}

impl MpoolSubscriber {
    /// Build a subscriber from a factory closure that yields a fresh
    /// receiver on each call (typically `move || mp.subscribe_to_updates()`).
    pub fn new<F>(factory: F) -> Self
    where
        F: Fn() -> broadcast::Receiver<MpoolUpdate> + Send + Sync + 'static,
    {
        Self {
            inner: Arc::new(factory),
        }
    }

    /// Subscriber whose receivers never receive any events. Used by
    /// standalone contexts (tests, snapshot tools, offline server when no
    /// real mempool is attached).
    pub fn dummy() -> Self {
        let (tx, _) = broadcast::channel::<MpoolUpdate>(1);
        Self::new(move || tx.subscribe())
    }

    fn subscribe(&self) -> broadcast::Receiver<MpoolUpdate> {
        (self.inner)()
    }
}

impl std::fmt::Debug for MpoolSubscriber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MpoolSubscriber").finish_non_exhaustive()
    }
}

/// Filter backing `eth_newPendingTransactionFilter`. Each instance owns its
/// own `broadcast::Receiver<MpoolUpdate>`.
#[derive(Debug)]
pub struct MempoolFilter {
    // Unique id used to identify the filter
    pub id: FilterID,
    // Maximum number of results to collect
    pub max_results: usize,
    // Receiver for mempool updates
    rx: Mutex<broadcast::Receiver<MpoolUpdate>>,
}

impl MempoolFilter {
    pub fn new(
        max_results: usize,
        rx: broadcast::Receiver<MpoolUpdate>,
    ) -> Result<Arc<Self>, uuid::Error> {
        Ok(Arc::new(Self {
            id: FilterID::new()?,
            max_results,
            rx: Mutex::new(rx),
        }))
    }

    /// Drain queued mempool updates and return the resulting set of pending
    /// tx hashes, capped at `max_results`.
    ///
    /// Semantics within a single drain window:
    /// - `Add` inserts the tx hash.
    /// - `Remove` cancels a prior `Add` from the *same* window. A `Remove`
    ///   for a hash that was already returned by an earlier `drain` call is
    ///   a no-op on the set — that hash was already reported as pending,
    ///   so the client has seen it and the cancellation does not need to
    ///   propagate.
    ///
    /// Why process `Remove` at all: a tx can leave the mempool between two
    /// client polls (mined into a tipset, replaced via RBF, or evicted). If
    /// we ignored `Remove` we would surface a hash whose tx is no longer
    /// pending, which is misleading for `eth_newPendingTransactionFilter`
    /// consumers.
    pub fn drain(&self, chain_id: EthChainIdType) -> Vec<EthHash> {
        use broadcast::error::TryRecvError;

        let mut rx = self.rx.lock();
        let mut pending: IndexSet<EthHash> = IndexSet::new();
        loop {
            match rx.try_recv() {
                Ok(MpoolUpdate::Add(m)) => {
                    if let Some(h) = hash_or_log(&m, chain_id) {
                        pending.insert(h);
                    }
                }
                Ok(MpoolUpdate::Remove(m)) => {
                    if let Some(h) = hash_or_log(&m, chain_id) {
                        // Cancels a matching Add buffered earlier in the
                        // same window. No-op if the hash is not in the set.
                        pending.shift_remove(&h);
                    }
                }
                Err(TryRecvError::Empty) | Err(TryRecvError::Closed) => break,
                Err(TryRecvError::Lagged(n)) => {
                    tracing::warn!("mempool filter lagged, dropped {n} events");
                }
            }
        }
        pending.into_iter().take(self.max_results).collect()
    }
}

impl Filter for MempoolFilter {
    fn id(&self) -> &FilterID {
        &self.id
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn hash_or_log(msg: &SignedMessage, chain_id: EthChainIdType) -> Option<EthHash> {
    match eth_tx_hash_from_signed_message(msg, chain_id) {
        Ok(h) => Some(h),
        Err(e) => {
            tracing::debug!("mempool filter: dropping message, hash error: {e}");
            None
        }
    }
}

/// Manages installed `MempoolFilter`s. Each `install` calls the configured
/// [`MpoolSubscriber`] to obtain a fresh independent
/// `broadcast::Receiver<MpoolUpdate>`. Contexts without a real `MessagePool`
/// (tests, snapshot tools, offline server) pass a subscriber whose receivers
/// always yield `Empty`.
pub struct MempoolFilterManager {
    filters: RwLock<HashMap<FilterID, Arc<dyn Filter>>>,
    max_filter_results: usize,
    subscriber: MpoolSubscriber,
}

impl std::fmt::Debug for MempoolFilterManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MempoolFilterManager")
            .field("max_filter_results", &self.max_filter_results)
            .finish_non_exhaustive()
    }
}

impl MempoolFilterManager {
    pub fn new(max_filter_results: usize, subscriber: MpoolSubscriber) -> Arc<Self> {
        Arc::new(Self {
            filters: RwLock::new(HashMap::new()),
            max_filter_results,
            subscriber,
        })
    }
}

impl FilterManager for MempoolFilterManager {
    fn install(&self) -> Result<Arc<dyn Filter>> {
        let filter = MempoolFilter::new(self.max_filter_results, self.subscriber.subscribe())
            .context("Failed to create a new mempool filter")?;
        self.filters
            .write()
            .insert(filter.id().clone(), filter.clone());
        Ok(filter)
    }

    fn remove(&self, id: &FilterID) -> Option<Arc<dyn Filter>> {
        self.filters.write().remove(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shim::address::Address;
    use crate::shim::econ::TokenAmount;
    use crate::shim::message::Message as ShimMessage;

    const TEST_CHAIN_ID: EthChainIdType = 314;

    fn make_smsg(seq: u64) -> SignedMessage {
        SignedMessage::mock_bls_signed_message(ShimMessage {
            from: Address::new_id(1),
            to: Address::new_id(2),
            sequence: seq,
            gas_premium: TokenAmount::from_atto(100u64),
            gas_limit: 1_000_000,
            ..ShimMessage::default()
        })
    }

    fn hash_of(seq: u64) -> EthHash {
        eth_tx_hash_from_signed_message(&make_smsg(seq), TEST_CHAIN_ID).unwrap()
    }

    /// Build a subscriber backed by `tx` so tests can drive
    /// `MpoolUpdate` events through the manager.
    fn subscriber_from(tx: broadcast::Sender<MpoolUpdate>) -> MpoolSubscriber {
        MpoolSubscriber::new(move || tx.subscribe())
    }

    #[test]
    fn drain_returns_empty_when_no_events() {
        let (tx, _) = broadcast::channel::<MpoolUpdate>(1);
        let filter = MempoolFilter::new(10, tx.subscribe()).unwrap();
        assert!(filter.drain(TEST_CHAIN_ID).is_empty());
    }

    #[test]
    fn drain_add_remove_cancel_within_window() {
        let (tx, _) = broadcast::channel::<MpoolUpdate>(16);
        let filter = MempoolFilter::new(10, tx.subscribe()).unwrap();

        tx.send(MpoolUpdate::Add(make_smsg(0))).unwrap();
        tx.send(MpoolUpdate::Add(make_smsg(1))).unwrap();
        tx.send(MpoolUpdate::Remove(make_smsg(0))).unwrap();
        tx.send(MpoolUpdate::Add(make_smsg(2))).unwrap();

        let hashes = filter.drain(TEST_CHAIN_ID);
        assert!(!hashes.contains(&hash_of(0)), "Add+Remove should cancel");
        assert!(hashes.contains(&hash_of(1)));
        assert!(hashes.contains(&hash_of(2)));
        assert!(filter.drain(TEST_CHAIN_ID).is_empty(), "second drain empty");
    }

    #[test]
    fn drain_truncates_to_max_results() {
        let (tx, _) = broadcast::channel::<MpoolUpdate>(64);
        let filter = MempoolFilter::new(2, tx.subscribe()).unwrap();
        for seq in 0..5u64 {
            tx.send(MpoolUpdate::Add(make_smsg(seq))).unwrap();
        }
        assert_eq!(filter.drain(TEST_CHAIN_ID).len(), 2);
    }

    #[test]
    fn drain_handles_lag_and_returns_remaining() {
        let (tx, _) = broadcast::channel::<MpoolUpdate>(4);
        let filter = MempoolFilter::new(100, tx.subscribe()).unwrap();
        for seq in 0..10u64 {
            tx.send(MpoolUpdate::Add(make_smsg(seq))).unwrap();
        }
        // Buffer was 4; receiver lagged. Drain returns the remaining buffered
        // events without panicking.
        assert!(!filter.drain(TEST_CHAIN_ID).is_empty());
    }

    #[test]
    fn manager_subscribes_each_filter_to_independent_receiver() {
        let (tx, _) = broadcast::channel::<MpoolUpdate>(16);
        let manager = MempoolFilterManager::new(100, subscriber_from(tx.clone()));

        let f1 = manager.install().expect("install f1");
        let f2 = manager.install().expect("install f2");

        tx.send(MpoolUpdate::Add(make_smsg(0))).unwrap();
        tx.send(MpoolUpdate::Add(make_smsg(1))).unwrap();

        let f1 = f1.as_any().downcast_ref::<MempoolFilter>().unwrap();
        let f2 = f2.as_any().downcast_ref::<MempoolFilter>().unwrap();

        // Each receiver sees the full broadcast, independently.
        let h1 = f1.drain(TEST_CHAIN_ID);
        let h2 = f2.drain(TEST_CHAIN_ID);
        assert_eq!(h1.len(), 2);
        assert_eq!(h2.len(), 2);

        // Draining once empties only that receiver.
        assert!(f1.drain(TEST_CHAIN_ID).is_empty());
    }

    #[test]
    fn manager_with_dummy_subscriber_yields_empty() {
        let manager = MempoolFilterManager::new(100, MpoolSubscriber::dummy());
        let f = manager.install().expect("install");
        let f = f.as_any().downcast_ref::<MempoolFilter>().unwrap();
        assert!(f.drain(TEST_CHAIN_ID).is_empty());
    }
}
