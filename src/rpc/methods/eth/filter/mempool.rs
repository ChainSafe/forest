// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::eth::EthChainId as EthChainIdType;
use crate::message::SignedMessage;
use crate::message_pool::MpoolUpdate;
use crate::rpc::Arc;
use crate::rpc::eth::eth_hash_from_signed_message;
use crate::rpc::eth::types::EthHash;
use crate::rpc::eth::{FilterID, filter::Filter, filter::FilterManager};
use ahash::AHashMap as HashMap;
use anyhow::{Context, Result};
use indexmap::IndexSet;
use parking_lot::{Mutex, RwLock};
use std::any::Any;
use tokio::sync::broadcast;

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
    /// tx hashes (`Add` minus subsequent `Remove`), capped at `max_results`.
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
    match eth_hash_from_signed_message(msg, chain_id) {
        Ok(h) => Some(h),
        Err(e) => {
            tracing::debug!("mempool filter: dropping message, hash error: {e}");
            None
        }
    }
}

/// Manages installed `MempoolFilter`s. Each `install` derives a fresh
/// `broadcast::Receiver` from the shared sender. Contexts without a real
/// `MessagePool` (tests, snapshot tools, offline server) pass a dummy sender
/// whose receivers always yield `Empty`.
#[derive(Debug)]
pub struct MempoolFilterManager {
    filters: RwLock<HashMap<FilterID, Arc<dyn Filter>>>,
    max_filter_results: usize,
    mpool_event_sender: broadcast::Sender<MpoolUpdate>,
}

impl MempoolFilterManager {
    pub fn new(
        max_filter_results: usize,
        mpool_event_sender: broadcast::Sender<MpoolUpdate>,
    ) -> Arc<Self> {
        Arc::new(Self {
            filters: RwLock::new(HashMap::new()),
            max_filter_results,
            mpool_event_sender,
        })
    }
}

impl FilterManager for MempoolFilterManager {
    fn install(&self) -> Result<Arc<dyn Filter>> {
        let rx = self.mpool_event_sender.subscribe();
        let filter = MempoolFilter::new(self.max_filter_results, rx)
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
        eth_hash_from_signed_message(&make_smsg(seq), TEST_CHAIN_ID).unwrap()
    }

    fn dummy_sender() -> broadcast::Sender<MpoolUpdate> {
        let (tx, _) = broadcast::channel(1);
        tx
    }

    #[test]
    fn drain_returns_empty_when_no_events() {
        let tx = dummy_sender();
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
}
