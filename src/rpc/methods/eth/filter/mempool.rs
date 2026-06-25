// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::eth::EthChainId as EthChainIdType;
use crate::message_pool::{MpoolSubscriber, MpoolUpdate};
use crate::prelude::*;
use crate::rpc::Arc;
use crate::rpc::eth::eth_tx_hash_from_signed_message;
use crate::rpc::eth::types::EthHash;
use crate::rpc::eth::{FilterID, filter::Filter, filter::FilterManager};
use crate::utils::broadcast::subscription_stream;
use crate::utils::task::AbortHandles;
use ahash::HashMap;
use anyhow::{Context, Result};
use futures::{Stream, StreamExt as _};
use indexmap::IndexSet;
use parking_lot::{Mutex, RwLock};
use std::any::Any;
use std::pin::Pin;
use tokio::sync::broadcast;

/// Stream of the eth tx hash for every [`MpoolUpdate::Add`] published on the
/// mempool bus.
///
/// Shared by the two pending-transaction surfaces — `eth_subscribe`'s
/// `newPendingTransactions` (see [`super::super::pubsub`]) and
/// `eth_newPendingTransactionFilter` — so both derive identical hashes by
/// construction and treat the feed as purely additive.
///
/// [`MpoolUpdate::Remove`] is ignored: a tx leaves the pool only once it is
/// mined on-chain, and — like Lotus and Forest's own pending-tx subscription —
/// neither surface retracts an already-reported pending hash. Lagged and closed
/// receivers are handled by [`subscription_stream`].
pub(crate) fn pending_tx_added_hashes(
    rx: broadcast::Receiver<MpoolUpdate>,
    eth_chain_id: EthChainIdType,
) -> Pin<Box<dyn Stream<Item = EthHash> + Send>> {
    subscription_stream(rx)
        .filter_map(move |update| async move {
            let MpoolUpdate::Add(msg) = update else {
                return None;
            };
            eth_tx_hash_from_signed_message(&msg, eth_chain_id)
                .inspect_err(|e| {
                    tracing::error!("Failed to compute eth tx hash from mpool message: {e:#}")
                })
                .ok()
        })
        .boxed()
}

/// Pending-tx hashes a [`MempoolFilter`] has accumulated since the last poll.
///
/// Insertion-ordered and de-duplicated. Bounded at `cap` hashes (`0` = no limit,
/// the subsystem-wide convention — see [`super::ensure_filter_cap`]); on overflow
/// the oldest hash is evicted, matching Lotus's mempool filter.
#[derive(Debug)]
struct Collected {
    hashes: IndexSet<EthHash>,
    cap: usize,
}

impl Collected {
    fn new(cap: usize) -> Self {
        Self {
            hashes: IndexSet::new(),
            cap,
        }
    }

    /// Record a newly-seen pending tx hash. Duplicates are ignored; on overflow
    /// the oldest hash is dropped to stay within `cap`.
    fn push(&mut self, hash: EthHash) {
        self.hashes.insert(hash);
        if self.cap != 0 && self.hashes.len() > self.cap {
            self.hashes.shift_remove_index(0);
        }
    }

    /// Take everything collected since the previous call, leaving the set empty.
    fn take(&mut self) -> Vec<EthHash> {
        std::mem::take(&mut self.hashes).into_iter().collect()
    }
}

/// Filter backing `eth_newPendingTransactionFilter`.
///
/// A background task drains the mempool [`MpoolUpdate`] bus continuously into
/// `collected`, mirroring Lotus's `WaitForMpoolUpdates`/`CollectMessage`. Filling
/// the buffer *between* polls (rather than reading the bounded broadcast ring at
/// poll time) is what lets a poll return up to `max_filter_results` hashes
/// instead of just what happens to still be in the ring.
/// [`drain`](Self::drain) then takes and clears whatever accumulated since the
/// previous `eth_getFilterChanges`. The feed is additive — see
/// [`pending_tx_added_hashes`].
#[derive(Debug)]
pub struct MempoolFilter {
    id: FilterID,
    collected: Arc<Mutex<Collected>>,
    /// Aborts the background drain task when the filter is dropped on uninstall.
    _drain_task: AbortHandles,
}

impl MempoolFilter {
    fn new(
        rx: broadcast::Receiver<MpoolUpdate>,
        eth_chain_id: EthChainIdType,
        max_results: usize,
    ) -> Result<Arc<Self>, uuid::Error> {
        let collected = Arc::new(Mutex::new(Collected::new(max_results)));

        // Drain the bus into `collected` continuously, so polls are bounded by
        // `max_results` rather than the broadcast ring's capacity.
        let mut hashes = pending_tx_added_hashes(rx, eth_chain_id);
        let task = {
            let collected = collected.clone();
            tokio::spawn(async move {
                while let Some(hash) = hashes.next().await {
                    collected.lock().push(hash);
                }
            })
        };
        let mut drain_task = AbortHandles::default();
        drain_task.push(task.abort_handle());

        Ok(Arc::new(Self {
            id: FilterID::new()?,
            collected,
            _drain_task: drain_task,
        }))
    }

    /// Take the pending-tx hashes collected since the previous poll.
    pub fn drain(&self) -> Vec<EthHash> {
        self.collected.lock().take()
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

/// Manages installed [`MempoolFilter`]s. Each `install` opens a fresh independent
/// receiver on the shared [`MpoolSubscriber`] and spawns the filter's background
/// drain task. Contexts without a real `MessagePool` (tests, snapshot tools, the
/// offline server) pass a dummy subscriber whose receivers never yield events.
#[derive(Debug)]
pub struct MempoolFilterManager {
    filters: RwLock<HashMap<FilterID, Arc<dyn Filter>>>,
    max_filter_results: usize,
    eth_chain_id: EthChainIdType,
    subscriber: MpoolSubscriber,
}

impl MempoolFilterManager {
    pub fn new(
        max_filter_results: usize,
        eth_chain_id: EthChainIdType,
        subscriber: MpoolSubscriber,
    ) -> Arc<Self> {
        Arc::new(Self {
            filters: RwLock::new(HashMap::new()),
            max_filter_results,
            eth_chain_id,
            subscriber,
        })
    }
}

impl FilterManager for MempoolFilterManager {
    fn install(&self) -> Result<Arc<dyn Filter>> {
        let filter = MempoolFilter::new(
            self.subscriber.subscribe(),
            self.eth_chain_id,
            self.max_filter_results,
        )
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
    use crate::message::SignedMessage;
    use crate::shim::address::Address;
    use crate::shim::econ::TokenAmount;
    use crate::shim::message::Message as ShimMessage;
    use std::time::Duration;

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

    /// Poll the filter until it has yielded at least `n` hashes in total,
    /// accumulating across polls. Avoids racing the background drain task, which
    /// collects asynchronously after an event is published.
    async fn collect_at_least(filter: &MempoolFilter, n: usize) -> Vec<EthHash> {
        let mut all = Vec::new();
        for _ in 0..200 {
            all.extend(filter.drain());
            if all.len() >= n {
                return all;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("collected only {} of {n} expected hashes", all.len());
    }

    // ---- Collected: the per-filter buffer logic (pure, deterministic) ----

    #[test]
    fn collected_dedups_and_preserves_insertion_order() {
        let mut c = Collected::new(0);
        c.push(hash_of(0));
        c.push(hash_of(1));
        c.push(hash_of(0)); // duplicate — ignored, keeps original position
        assert_eq!(c.take(), vec![hash_of(0), hash_of(1)]);
    }

    #[test]
    fn collected_take_clears_the_buffer() {
        let mut c = Collected::new(0);
        c.push(hash_of(0));
        assert_eq!(c.take(), vec![hash_of(0)]);
        assert!(
            c.take().is_empty(),
            "a second take with no new pushes is empty"
        );
    }

    #[test]
    fn collected_evicts_oldest_at_cap() {
        let mut c = Collected::new(2);
        c.push(hash_of(0));
        c.push(hash_of(1));
        c.push(hash_of(2)); // overflow — oldest (0) evicted
        assert_eq!(c.take(), vec![hash_of(1), hash_of(2)]);
    }

    #[test]
    fn collected_cap_zero_means_unbounded() {
        let mut c = Collected::new(0);
        for seq in 0..10 {
            c.push(hash_of(seq));
        }
        assert_eq!(c.take().len(), 10, "cap == 0 never evicts");
    }

    // ---- pending_tx_added_hashes: the shared Add-only hash stream ----

    #[tokio::test]
    async fn added_hashes_maps_adds_and_ignores_removes() {
        let (tx, rx) = broadcast::channel::<MpoolUpdate>(16);
        let stream = pending_tx_added_hashes(rx, TEST_CHAIN_ID);

        tx.send(MpoolUpdate::Add(make_smsg(0))).unwrap();
        tx.send(MpoolUpdate::Remove(make_smsg(1))).unwrap(); // ignored
        tx.send(MpoolUpdate::Add(make_smsg(2))).unwrap();
        drop(tx); // close the channel so the stream terminates

        let hashes: Vec<EthHash> = stream.collect().await;
        assert_eq!(hashes, vec![hash_of(0), hash_of(2)]);
    }

    // ---- MempoolFilter / manager wiring ----

    #[tokio::test]
    async fn filter_collects_adds_from_its_receiver() {
        let (tx, rx) = broadcast::channel::<MpoolUpdate>(16);
        let filter = MempoolFilter::new(rx, TEST_CHAIN_ID, 100).unwrap();

        tx.send(MpoolUpdate::Add(make_smsg(0))).unwrap();
        tx.send(MpoolUpdate::Add(make_smsg(1))).unwrap();

        let hashes = collect_at_least(&filter, 2).await;
        assert!(hashes.contains(&hash_of(0)));
        assert!(hashes.contains(&hash_of(1)));
    }

    #[tokio::test]
    async fn manager_installs_and_removes_filters() {
        let manager = MempoolFilterManager::new(100, TEST_CHAIN_ID, MpoolSubscriber::dummy());
        let filter = manager.install().expect("install");
        let id = filter.id().clone();
        assert!(manager.remove(&id).is_some());
        assert!(manager.remove(&id).is_none(), "second remove finds nothing");
    }

    #[tokio::test]
    async fn dummy_subscriber_yields_no_hashes() {
        // A standalone handler (no live mempool) installs fine but never collects.
        let manager = MempoolFilterManager::new(100, TEST_CHAIN_ID, MpoolSubscriber::dummy());
        let filter = manager.install().expect("install");
        let filter = filter.as_any().downcast_ref::<MempoolFilter>().unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(filter.drain().is_empty());
    }
}
