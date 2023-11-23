// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Request manager implementation that is optimized for `filecoin` network
//! usage

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use crate::cid_collections::CidHashMap;
use ahash::{HashSet, HashSetExt};
use flume::TryRecvError;
use futures::StreamExt;
use libipld::{Block, Cid};
use libp2p::PeerId;
use parking_lot::RwLock;

use crate::libp2p_bitswap::{event_handlers::*, *};

const BITSWAP_BLOCK_REQUEST_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone)]
struct ResponseChannels {
    block_have: flume::Sender<PeerId>,
    block_received: flume::Sender<Option<Vec<u8>>>,
}

/// Request manager implementation that is optimized for Filecoin network
/// usage
#[derive(Debug)]
pub struct BitswapRequestManager {
    // channel for outbound `have` requests
    outbound_have_request_tx: flume::Sender<(PeerId, Cid)>,
    outbound_have_request_rx: flume::Receiver<(PeerId, Cid)>,
    // channel for outbound `cancel` requests
    outbound_cancel_request_tx: flume::Sender<(PeerId, Cid)>,
    outbound_cancel_request_rx: flume::Receiver<(PeerId, Cid)>,
    // channel for outbound `block` requests
    outbound_block_request_tx: flume::Sender<(PeerId, Cid)>,
    outbound_block_request_rx: flume::Receiver<(PeerId, Cid)>,
    peers: RwLock<HashSet<PeerId>>,
    response_channels: RwLock<CidHashMap<ResponseChannels>>,
}

impl BitswapRequestManager {
    /// A receiver channel of the outbound `bitswap` network requests that the
    /// [`BitswapRequestManager`] emits. The messages from this channel need
    /// to be sent with [`BitswapBehaviour::send_request`] to make
    /// [`BitswapRequestManager::get_block`] work.
    pub fn outbound_request_stream(
        &self,
    ) -> impl futures::stream::Stream<Item = (PeerId, BitswapRequest)> + '_ {
        type MapperType = fn((libp2p::PeerId, Cid)) -> (libp2p::PeerId, BitswapRequest);

        fn new_block((peer, cid): (PeerId, Cid)) -> (PeerId, BitswapRequest) {
            (peer, BitswapRequest::new_block(cid).send_dont_have(false))
        }

        fn new_have((peer, cid): (PeerId, Cid)) -> (PeerId, BitswapRequest) {
            (peer, BitswapRequest::new_have(cid).send_dont_have(false))
        }

        fn new_cancel((peer, cid): (PeerId, Cid)) -> (PeerId, BitswapRequest) {
            (peer, BitswapRequest::new_cancel(cid).send_dont_have(false))
        }

        // Use seperate channels here to not block `block` requests when too many other type of requests are queued.
        let streams = vec![
            self.outbound_block_request_rx
                .stream()
                .map(new_block as MapperType),
            self.outbound_have_request_rx
                .stream()
                .map(new_have as MapperType),
            self.outbound_cancel_request_rx
                .stream()
                .map(new_cancel as MapperType),
        ];
        futures::stream::select_all(streams)
    }
}

impl Default for BitswapRequestManager {
    fn default() -> Self {
        let (outbound_have_request_tx, outbound_have_request_rx) = flume::unbounded();
        let (outbound_cancel_request_tx, outbound_cancel_request_rx) = flume::unbounded();
        let (outbound_block_request_tx, outbound_block_request_rx) = flume::unbounded();
        Self {
            outbound_have_request_tx,
            outbound_have_request_rx,
            outbound_cancel_request_tx,
            outbound_cancel_request_rx,
            outbound_block_request_tx,
            outbound_block_request_rx,
            peers: RwLock::new(HashSet::new()),
            response_channels: RwLock::new(CidHashMap::new()),
        }
    }
}

impl BitswapRequestManager {
    /// Hook the `bitswap` network event into the [`BitswapRequestManager`]
    pub fn handle_event<S: BitswapStoreRead>(
        self: &Arc<Self>,
        bitswap: &mut BitswapBehaviour,
        store: &S,
        event: BitswapBehaviourEvent,
    ) -> anyhow::Result<()> {
        handle_event_impl(self, bitswap, store, event)
    }

    /// Gets a block, writing it to the given block store that implements
    /// [`BitswapStoreReadWrite`] and respond to the channel. Note: this
    /// method is a non-blocking, it is intended to return immediately.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn get_block(
        self: Arc<Self>,
        store: Arc<impl BitswapStoreReadWrite>,
        cid: Cid,
        timeout: Duration,
        responder: Option<flume::Sender<bool>>,
    ) {
        let start = Instant::now();
        let timer = metrics::GET_BLOCK_TIME.start_timer();
        let store_cloned = store.clone();
        task::spawn(async move {
            let mut success = store.contains(&cid).unwrap_or_default();
            if !success {
                let deadline = start.checked_add(timeout).expect("Infallible");
                success =
                    task::spawn_blocking(move || self.get_block_sync(store_cloned, cid, deadline))
                        .await
                        .unwrap_or_default();
                // Spin check db when `get_block_sync` fails fast,
                // which means there is other task actually processing the same `cid`
                while !success && Instant::now() < deadline {
                    task::sleep(BITSWAP_BLOCK_REQUEST_INTERVAL).await;
                    success = store.contains(&cid).unwrap_or_default();
                }
            }

            if success {
                metrics::message_counter_get_block_success().inc();
            } else {
                metrics::message_counter_get_block_failure().inc();
            }

            if let Some(responder) = responder {
                if let Err(e) = responder.send_async(success).await {
                    warn!("{e}");
                }
            }

            timer.observe_duration();
        });
    }

    fn get_block_sync(
        &self,
        store: Arc<impl BitswapStoreReadWrite>,
        cid: Cid,
        deadline: Instant,
    ) -> bool {
        // Fail fast here when the given `cid` is being processed by other tasks
        if self.response_channels.read().contains_key(&cid) {
            return false;
        }

        let (block_have_tx, block_have_rx) = flume::unbounded();
        let (block_saved_tx, block_saved_rx) = flume::unbounded();
        let channels = ResponseChannels {
            block_have: block_have_tx,
            block_received: block_saved_tx,
        };
        {
            self.response_channels.write().insert(cid, channels);
        }

        for &peer in self.peers.read().iter() {
            if let Err(e) = self.outbound_have_request_tx.send((peer, cid)) {
                warn!("{e}");
            }
        }

        let mut success = false;
        let mut block_data = None;
        while !success && Instant::now() < deadline {
            match block_have_rx.try_recv() {
                Ok(peer) => {
                    _ = self.outbound_block_request_tx.send((peer, cid));
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    break;
                }
            }

            if let Ok(data) = block_saved_rx.recv_timeout(BITSWAP_BLOCK_REQUEST_INTERVAL) {
                success = true;
                block_data = data;
            }
        }

        if !success {
            if let Ok(data) = block_saved_rx.recv_deadline(deadline) {
                success = true;
                block_data = data;
            }
        }

        if let Some(data) = block_data {
            success = match Block::new(cid, data) {
                Ok(block) => match store.insert(&block) {
                    Ok(()) => {
                        metrics::message_counter_inbound_response_block_update_db().inc();
                        true
                    }
                    Err(e) => {
                        metrics::message_counter_inbound_response_block_update_db_failure().inc();
                        warn!(
                            "Failed to update db: {e}, cid: {cid}, data: {:?}",
                            block.data()
                        );
                        false
                    }
                },
                Err(e) => {
                    warn!("Failed to construct block: {e}, cid: {cid}");
                    false
                }
            };
        }

        // Cleanup
        {
            let mut response_channels = self.response_channels.write();
            response_channels.remove(&cid);
            metrics::response_channel_container_capacity()
                .set(response_channels.total_capacity() as _);
        }

        success
    }

    pub(in crate::libp2p_bitswap) fn on_inbound_response_event<S: BitswapStoreRead>(
        &self,
        store: &S,
        response: BitswapInboundResponseEvent,
    ) {
        use BitswapInboundResponseEvent::*;

        match response {
            HaveBlock(peer, cid) => {
                if let Some(chans) = self.response_channels.read().get(&cid) {
                    _ = chans.block_have.send(peer);
                }
            }
            DataBlock(_peer, cid, data) => {
                if let Some(chans) = self.response_channels.read().get(&cid) {
                    if let Ok(true) = store.contains(&cid) {
                        // Avoid duplicate writes, still notify the receiver
                        metrics::message_counter_inbound_response_block_already_exists_in_db()
                            .inc();
                        _ = chans.block_received.send(None);
                    } else {
                        _ = chans.block_received.send(Some(data));
                    }

                    // <https://github.com/ipfs/go-libipfs/tree/main/bitswap#background>
                    // When a node receives blocks that it asked for, the node should send out a
                    // notification called a 'Cancel' to tell its peers that the
                    // node no longer wants those blocks.
                    for &peer in self.peers.read().iter() {
                        if let Err(e) = self.outbound_cancel_request_tx.send((peer, cid)) {
                            warn!("{e}");
                        }
                    }
                } else {
                    metrics::message_counter_inbound_response_block_not_requested().inc();
                }
            }
        }
    }

    pub(in crate::libp2p_bitswap) fn on_peer_connected(&self, peer: PeerId) -> bool {
        let mut peers = self.peers.write();
        let success = peers.insert(peer);
        if success {
            metrics::peer_container_capacity().set(peers.capacity() as _);
        }
        success
    }

    pub(in crate::libp2p_bitswap) fn on_peer_disconnected(&self, peer: &PeerId) -> bool {
        let mut peers = self.peers.write();
        let success = peers.remove(peer);
        if success {
            metrics::peer_container_capacity().set(peers.capacity() as _);
        }
        success
    }
}
