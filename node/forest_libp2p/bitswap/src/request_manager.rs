// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{event_handlers::*, *};
use ahash::{HashMap, HashMapExt, HashSet, HashSetExt};
use flume::TryRecvError;
use libipld::Block;
use libipld::Cid;
use libp2p::PeerId;
use parking_lot::RwLock;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

const BITSWAP_BLOCK_REQUEST_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone)]
struct ResponseChannels {
    block_have: flume::Sender<PeerId>,
    block_saved: flume::Sender<()>,
}

// TODO: Use message queue like `go-bitswap`
#[derive(Debug)]
pub struct BitswapRequestManager {
    outbound_request_tx: flume::Sender<(PeerId, BitswapRequest)>,
    peers: RwLock<HashSet<PeerId>>,
    response_channels: RwLock<HashMap<Cid, ResponseChannels>>,
}

impl BitswapRequestManager {
    pub fn new(outbound_request_tx: flume::Sender<(PeerId, BitswapRequest)>) -> Self {
        Self {
            outbound_request_tx,
            peers: RwLock::new(HashSet::new()),
            response_channels: RwLock::new(HashMap::new()),
        }
    }
}

impl BitswapRequestManager {
    pub fn on_peer_connected(&self, peer: PeerId) -> bool {
        let mut peers = self.peers.write();
        let success = peers.insert(peer);
        if success {
            metrics::peer_container_capacity().set(peers.capacity() as _);
        }
        success
    }

    pub fn on_peer_disconnected(&self, peer: &PeerId) -> bool {
        let mut peers = self.peers.write();
        let success = peers.remove(peer);
        if success {
            metrics::peer_container_capacity().set(peers.capacity() as _);
        }
        success
    }

    pub fn handle_event<S: BitswapStore>(
        self: &Arc<Self>,
        bitswap: &mut BitswapBehaviour,
        store: &S,
        event: BitswapBehaviourEvent,
    ) -> anyhow::Result<()> {
        handle_event_impl(self, bitswap, store, event)
    }

    pub fn get_block(
        self: Arc<Self>,
        store: Arc<impl BitswapStore>,
        cid: Cid,
        timeout: Duration,
        responder: Option<flume::Sender<bool>>,
    ) {
        let start = Instant::now();
        let timer = metrics::GET_BLOCK_TIME.start_timer();
        tokio::spawn(async move {
            let mut success = store.contains(&cid).unwrap_or_default();
            if !success {
                let deadline = start.checked_add(timeout).expect("Infallible");
                success = tokio::task::spawn_blocking(move || self.get_block_sync(cid, deadline))
                    .await
                    .unwrap_or_default();
                // Spin check db when `get_block_sync` fails fast,
                // which means there is other task actually processing the same `cid`
                while !success && Instant::now() < deadline {
                    tokio::time::sleep(BITSWAP_BLOCK_REQUEST_INTERVAL).await;
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

    fn get_block_sync(&self, cid: Cid, deadline: Instant) -> bool {
        // Fail fast here when the given `cid` is being processed by other tasks
        if self.response_channels.read().contains_key(&cid) {
            return false;
        }

        let (block_have_tx, block_have_rx) = flume::unbounded();
        let (block_saved_tx, block_saved_rx) = flume::unbounded();
        let channels = ResponseChannels {
            block_have: block_have_tx,
            block_saved: block_saved_tx,
        };
        {
            self.response_channels.write().insert(cid, channels);
        }

        let have_request = BitswapRequest::new_have(cid).send_dont_have(false);
        for &peer in self.peers.read().iter() {
            if let Err(e) = self.outbound_request_tx.send((peer, have_request.clone())) {
                warn!("{e}");
            }
        }

        let mut success = false;
        let block_request = BitswapRequest::new_block(cid).send_dont_have(false);
        while !success && Instant::now() < deadline {
            match block_have_rx.try_recv() {
                Ok(peer) => {
                    _ = self.outbound_request_tx.send((peer, block_request.clone()));
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    break;
                }
            }

            if let Ok(()) = block_saved_rx.recv_timeout(BITSWAP_BLOCK_REQUEST_INTERVAL) {
                success = true;
            }
        }

        if !success {
            success = block_saved_rx.recv_deadline(deadline).is_ok();
        }

        // Cleanup
        {
            let mut response_channels = self.response_channels.write();
            response_channels.remove(&cid);
            metrics::response_channel_container_capacity().set(response_channels.capacity() as _);
        }

        success
    }

    pub(crate) fn on_inbound_response_event<S: BitswapStore>(
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
                        _ = chans.block_saved.send(());
                        _ = chans.block_saved.send(());
                    } else {
                        match Block::new(cid, data) {
                            Ok(block) => match store.insert(&block) {
                                Ok(()) => {
                                    metrics::message_counter_inbound_response_block_update_db()
                                        .inc();
                                    _ = chans.block_saved.send(());
                                }
                                Err(e) => {
                                    metrics::message_counter_inbound_response_block_update_db_failure()
                                    .inc();
                                    warn!(
                                        "Failed to update db: {e}, cid: {cid}, data: {:?}",
                                        block.data()
                                    );
                                }
                            },
                            Err(e) => {
                                // TODO: log data
                                warn!("Failed to construct block: {e}, cid: {cid}");
                            }
                        }
                    }
                } else {
                    metrics::message_counter_inbound_response_block_not_requested().inc();
                }
            }
        }
    }
}
