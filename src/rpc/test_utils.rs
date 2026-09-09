// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Fixtures for building an [`RPCState`] in tests.

use crate::blocks::{CachingBlockHeader, RawBlockHeader};
use crate::chain::ChainStore;
use crate::chain_sync::network_context::SyncNetworkContext;
use crate::db::MemoryDB;
use crate::key_management::{KeyStore, KeyStoreConfig};
use crate::libp2p::{NetworkMessage, PeerManager};
use crate::message_pool::{MessagePool, MpoolLocker, NonceTracker};
use crate::networks::ChainConfig;
use crate::rpc::RPCState;
use crate::rpc::eth::filter::EthEventHandler;
use crate::state_manager::StateManager;
use crate::test_utils::dummy_ticket;
use crate::utils::ShallowClone as _;
use crate::utils::db::CborStoreExt as _;
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

/// A chain store over a fresh in-memory db, with its genesis persisted.
pub(crate) fn chain_store() -> ChainStore {
    let db = Arc::new(MemoryDB::default());
    let genesis = CachingBlockHeader::new(RawBlockHeader {
        timestamp: 7777,
        // Tipsets must carry a ticket to be loadable by key.
        ticket: dummy_ticket(0),
        ..Default::default()
    });
    db.put_cbor_default(&genesis).unwrap();
    ChainStore::new(db, Arc::new(ChainConfig::default()), genesis).unwrap()
}

impl RPCState {
    /// An [`RPCState`] over `chain_store`, with a real message pool that adopts the store's
    /// current heaviest tipset. Set the heaviest tipset before calling.
    pub(crate) fn for_tests(
        chain_store: ChainStore,
    ) -> anyhow::Result<(Arc<Self>, flume::Receiver<NetworkMessage>)> {
        let (network_send, network_rx) = flume::bounded(5);
        let (tipset_send, _) = flume::bounded(5);
        let state_manager = StateManager::new(chain_store.shallow_clone())?;
        let mut services = JoinSet::new();
        let mpool = MessagePool::new(
            chain_store,
            network_send.clone(),
            Default::default(),
            state_manager.chain_config().clone(),
            &mut services,
        )?;
        // Keep the pool's background tasks running past this scope; the test runtime reaps them.
        services.detach_all();
        let state = Arc::new(Self {
            keystore: Arc::new(RwLock::new(KeyStore::new(KeyStoreConfig::Memory)?)),
            sync_network_context: SyncNetworkContext::new(
                network_send,
                Arc::new(PeerManager::default()),
                state_manager.db_owned(),
            ),
            state_manager,
            mpool,
            bad_blocks: Some(Default::default()),
            sync_status: Default::default(),
            eth_event_handler: Arc::new(EthEventHandler::new()),
            eth_logs_feed: Default::default(),
            start_time: chrono::Utc::now(),
            shutdown: mpsc::channel(1).0,
            tipset_send,
            block_validation_subscriber: Default::default(),
            snapshot_progress_tracker: Default::default(),
            mpool_locker: MpoolLocker::new(),
            nonce_tracker: NonceTracker::new(),
            temp_dir: Arc::new(std::env::temp_dir()),
        });
        Ok((state, network_rx))
    }
}
