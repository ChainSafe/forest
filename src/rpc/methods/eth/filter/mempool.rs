// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::eth::EthChainId as EthChainIdType;
use crate::message_pool::{MpoolSubscriber, MpoolUpdate};
use crate::prelude::ShallowClone;
use crate::rpc::Arc;
use crate::rpc::eth::eth_tx_hash_from_signed_message;
use crate::rpc::eth::types::EthHash;
use crate::rpc::eth::{FilterID, filter::Filter, filter::FilterManager};
use crate::utils::broadcast::subscription_stream;
use ahash::HashMap;
use anyhow::{Context, Result};
use futures::{Stream, StreamExt as _};
use parking_lot::{Mutex, RwLock};
use std::any::Any;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::OnceLock;
use tokio::sync::broadcast;
use tokio_util::task::AbortOnDropHandle;

/// Stream of the eth tx hash for every [`MpoolUpdate::Add`]; `Remove`s are skipped.
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

/// A bounded FIFO of pending-tx hashes, backing `eth_newPendingTransactionFilter`.
#[derive(Debug)]
pub struct MempoolFilter {
    id: FilterID,
    hashes: Mutex<VecDeque<EthHash>>,
    cap: usize,
}

impl MempoolFilter {
    fn new(max_results: usize) -> Result<Arc<Self>, uuid::Error> {
        Ok(Arc::new(Self {
            id: FilterID::new()?,
            hashes: Mutex::new(VecDeque::new()),
            cap: max_results,
        }))
    }

    /// Append a hash, evicting the oldest when over `cap`.
    fn push(&self, hash: EthHash) {
        let mut hashes = self.hashes.lock();
        hashes.push_back(hash);
        if self.cap != 0 && hashes.len() > self.cap {
            hashes.pop_front();
        }
    }

    /// Take and clear the hashes collected since the last poll.
    pub fn drain(&self) -> Vec<EthHash> {
        self.hashes.lock().drain(..).collect()
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

/// Push `hash` into every installed filter.
fn fan_out(filters: &HashMap<FilterID, Arc<MempoolFilter>>, hash: EthHash) {
    for filter in filters.values() {
        filter.push(hash);
    }
}

/// Manages installed [`MempoolFilter`]s and the single fan-out task that drains
/// the mempool bus and pushes each pending-tx hash into every filter.
#[derive(Debug)]
pub struct MempoolFilterManager {
    filters: Arc<RwLock<HashMap<FilterID, Arc<MempoolFilter>>>>,
    eth_chain_id: EthChainIdType,
    max_filter_results: usize,
    subscriber: MpoolSubscriber,
    /// Aborts the fan-out task when the manager is dropped.
    fanout_task: OnceLock<AbortOnDropHandle<()>>,
}

impl MempoolFilterManager {
    pub fn new(
        max_filter_results: usize,
        eth_chain_id: EthChainIdType,
        subscriber: MpoolSubscriber,
    ) -> Arc<Self> {
        Arc::new(Self {
            filters: Arc::new(RwLock::new(HashMap::default())),
            eth_chain_id,
            max_filter_results,
            subscriber,
            fanout_task: OnceLock::new(),
        })
    }

    /// Lazily start the fan-out task on the first install.
    fn ensure_fanout_task(&self) {
        self.fanout_task.get_or_init(|| {
            let filters = self.filters.shallow_clone();
            let mut hashes =
                pending_tx_added_hashes(self.subscriber.subscribe(), self.eth_chain_id);
            AbortOnDropHandle::new(tokio::spawn(async move {
                while let Some(hash) = hashes.next().await {
                    fan_out(&filters.read(), hash);
                }
            }))
        });
    }
}

impl FilterManager for MempoolFilterManager {
    fn install(&self) -> Result<Arc<dyn Filter>> {
        self.ensure_fanout_task();
        let filter = MempoolFilter::new(self.max_filter_results)
            .context("Failed to create a new mempool filter")?;
        self.filters
            .write()
            .insert(filter.id().clone(), filter.clone());
        Ok(filter)
    }

    fn remove(&self, id: &FilterID) -> Option<Arc<dyn Filter>> {
        self.filters
            .write()
            .remove(id)
            .map(|f| -> Arc<dyn Filter> { f })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::SignedMessage;
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

    #[test]
    fn filter_keeps_duplicates_in_order() {
        // a re-added tx is a distinct event; duplicates are kept (like reth/Lotus/geth)
        let f = MempoolFilter::new(0).unwrap();
        f.push(hash_of(0));
        f.push(hash_of(1));
        f.push(hash_of(0)); // same tx seen again — recorded again
        assert_eq!(f.drain(), vec![hash_of(0), hash_of(1), hash_of(0)]);
    }

    #[test]
    fn filter_drain_clears_the_buffer() {
        let f = MempoolFilter::new(0).unwrap();
        f.push(hash_of(0));
        assert_eq!(f.drain(), vec![hash_of(0)]);
        assert!(
            f.drain().is_empty(),
            "a second drain with no new pushes is empty"
        );
    }

    #[test]
    fn filter_evicts_oldest_at_cap() {
        let f = MempoolFilter::new(2).unwrap();
        f.push(hash_of(0));
        f.push(hash_of(1));
        f.push(hash_of(2)); // overflow — oldest (0) evicted
        assert_eq!(f.drain(), vec![hash_of(1), hash_of(2)]);
    }

    #[test]
    fn filter_cap_zero_means_unbounded() {
        let f = MempoolFilter::new(0).unwrap();
        for seq in 0..10 {
            f.push(hash_of(seq));
        }
        assert_eq!(f.drain().len(), 10, "cap == 0 never evicts");
    }

    #[test]
    fn fan_out_pushes_hash_to_every_filter() {
        let f1 = MempoolFilter::new(100).unwrap();
        let f2 = MempoolFilter::new(100).unwrap();
        let mut filters = HashMap::default();
        filters.insert(f1.id().clone(), f1.clone());
        filters.insert(f2.id().clone(), f2.clone());

        fan_out(&filters, hash_of(0));

        assert_eq!(f1.drain(), vec![hash_of(0)]);
        assert_eq!(f2.drain(), vec![hash_of(0)]);
    }

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

    #[tokio::test]
    async fn dispatcher_fans_out_bus_events_to_all_filters() {
        let (tx, _) = broadcast::channel::<MpoolUpdate>(16);
        let manager =
            MempoolFilterManager::new(100, TEST_CHAIN_ID, MpoolSubscriber::new(tx.clone()));
        let f1 = manager.install().expect("install f1");
        let f2 = manager.install().expect("install f2");

        tx.send(MpoolUpdate::Add(make_smsg(0))).unwrap();
        // The receiver is subscribed during install, so the event is already
        // buffered; one yield lets the fan-out task drain it into both filters.
        tokio::task::yield_now().await;

        let f1 = f1.as_any().downcast_ref::<MempoolFilter>().unwrap();
        let f2 = f2.as_any().downcast_ref::<MempoolFilter>().unwrap();
        assert_eq!(f1.drain(), vec![hash_of(0)]);
        assert_eq!(f2.drain(), vec![hash_of(0)]);
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
        let manager = MempoolFilterManager::new(100, TEST_CHAIN_ID, MpoolSubscriber::dummy());
        let filter = manager.install().expect("install");
        let filter = filter.as_any().downcast_ref::<MempoolFilter>().unwrap();
        tokio::task::yield_now().await; // let the task run; the dummy produces nothing
        assert!(filter.drain().is_empty());
    }
}
