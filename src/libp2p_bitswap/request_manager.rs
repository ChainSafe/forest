// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Request manager implementation that is optimized for `filecoin` network
//! usage

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use crate::cid_collections::CidHashMap;
use crate::prelude::*;
use crate::utils::misc::env::env_or_default_logged;
use ahash::HashSet;
use futures::StreamExt;
use libp2p::PeerId;
use nonzero_ext::nonzero;
use parking_lot::RwLock;
use tokio::sync::Semaphore;

use crate::libp2p_bitswap::{event_handlers::*, *};

const BITSWAP_BLOCK_REQUEST_INTERVAL: Duration = Duration::from_millis(500);

/// Bounds the queue of computed wantlist responses awaiting send by the swarm
/// loop. A `Block` response can carry up to
/// [`MAX_BUF_SIZE`](crate::libp2p_bitswap::internals::codec::MAX_BUF_SIZE), so an
/// unbounded queue could buffer gigabytes under a flood of block requests for
/// large stored blocks. When the channel is full the (already blocking) serve
/// task waits, holding its permit and so shedding further inbound serves.
const SERVE_RESPONSE_CHANNEL_CAP: usize = 128;

/// Maximum wantlist entries served from a single inbound bitswap message.
///
/// The peer picks the entry count, bounded only by
/// [`MAX_BUF_SIZE`](crate::libp2p_bitswap::internals::codec::MAX_BUF_SIZE) (~47k
/// entries), and each is a blockstore read; serving them all in one message is
/// an unbounded burst of DB IO. Well-behaved peers that want more simply
/// re-request.
pub(in crate::libp2p_bitswap) const MAX_WANTLIST_ENTRIES_SERVED: usize = 1024;

/// Concurrent inbound wantlist serves allowed at once. Each holds a blocking
/// thread while it reads the blockstore, so this bounds blocking-pool usage
/// under a flood. Excess serves are dropped; the peer can re-request.
pub(in crate::libp2p_bitswap) static MAX_CONCURRENT_INBOUND_WANTLIST_SERVES: LazyLock<usize> =
    LazyLock::new(|| {
        env_or_default_logged(
            "FOREST_MAX_CONCURRENT_INBOUND_WANTLIST_SERVES",
            nonzero!(8_usize),
        )
        .get()
    });

pub type ValidatePeerCallback = dyn Fn(PeerId) -> bool + Send + Sync;

#[derive(Debug, Clone)]
struct ResponseChannels {
    block_have: flume::Sender<PeerId>,
    block_received: flume::Sender<Option<Vec<u8>>>,
}

/// Request manager implementation that is optimized for Filecoin network
/// usage
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
    // responses to inbound wantlists, computed off the swarm loop and sent from it
    serve_response_tx: flume::Sender<(PeerId, Cid, BitswapResponse)>,
    serve_response_rx: flume::Receiver<(PeerId, Cid, BitswapResponse)>,
    // bounds concurrent off-loop wantlist serving
    inbound_serve_limiter: Arc<Semaphore>,
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

        // Use separate channels here to not block `block` requests when too many other type of requests are queued.
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

    /// Responses to inbound wantlists, computed off the swarm loop by
    /// [`Self::serve_inbound_requests`]. Each item must be sent with
    /// [`BitswapBehaviour::send_response`].
    pub fn outbound_serve_response_stream(
        &self,
    ) -> impl futures::stream::Stream<Item = (PeerId, Cid, BitswapResponse)> + '_ {
        self.serve_response_rx.stream()
    }

    /// Serves an inbound wantlist off the swarm loop: the blockstore reads run
    /// on a blocking task (bounded by [`MAX_CONCURRENT_INBOUND_WANTLIST_SERVES`],
    /// dropped when saturated) and the responses are streamed back via
    /// [`Self::outbound_serve_response_stream`] for the loop to send. Keeping the
    /// reads off the loop is what stops a large wantlist from stalling all p2p.
    pub(in crate::libp2p_bitswap) fn serve_inbound_requests<S>(
        self: &Arc<Self>,
        store: &S,
        peer: PeerId,
        requests: Vec<BitswapRequest>,
    ) where
        S: BitswapStoreRead + ShallowClone + Send + Sync + 'static,
    {
        if requests.is_empty() {
            return;
        }

        let Ok(permit) = self
            .inbound_serve_limiter
            .shallow_clone()
            .try_acquire_owned()
        else {
            debug!(%peer, "dropping inbound bitswap wantlist: too many serves in flight");
            return;
        };
        if requests.len() > MAX_WANTLIST_ENTRIES_SERVED {
            debug!(
                %peer,
                "truncating inbound bitswap wantlist from {} to {MAX_WANTLIST_ENTRIES_SERVED} entries",
                requests.len(),
            );
        }
        let store = store.shallow_clone();
        let serve_response_tx = self.serve_response_tx.clone();
        task::spawn_blocking(move || {
            let _permit = permit;
            for request in requests.into_iter().take(MAX_WANTLIST_ENTRIES_SERVED) {
                if let Some(response) = handle_inbound_request(&store, &request)
                    && serve_response_tx
                        .send((peer, request.cid, response))
                        .is_err()
                {
                    break; // receiver gone (shutdown)
                }
            }
        });
    }
}

impl Default for BitswapRequestManager {
    fn default() -> Self {
        let (outbound_have_request_tx, outbound_have_request_rx) = flume::unbounded();
        let (outbound_cancel_request_tx, outbound_cancel_request_rx) = flume::unbounded();
        let (outbound_block_request_tx, outbound_block_request_rx) = flume::unbounded();
        let (serve_response_tx, serve_response_rx) = flume::bounded(SERVE_RESPONSE_CHANNEL_CAP);
        Self {
            outbound_have_request_tx,
            outbound_have_request_rx,
            outbound_cancel_request_tx,
            outbound_cancel_request_rx,
            outbound_block_request_tx,
            outbound_block_request_rx,
            serve_response_tx,
            serve_response_rx,
            inbound_serve_limiter: Arc::new(Semaphore::new(
                *MAX_CONCURRENT_INBOUND_WANTLIST_SERVES,
            )),
            peers: RwLock::new(HashSet::new()),
            response_channels: RwLock::new(CidHashMap::new()),
        }
    }
}

impl BitswapRequestManager {
    /// Hook the `bitswap` network event into the [`BitswapRequestManager`]
    pub fn handle_event<S: BitswapStoreRead + ShallowClone + Send + Sync + 'static>(
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
        store: impl BitswapStoreReadWrite + ShallowClone,
        cid: Cid,
        timeout: Duration,
        responder: Option<flume::Sender<bool>>,
        validate_peer: Option<Arc<ValidatePeerCallback>>,
    ) {
        let start = Instant::now();
        task::spawn(async move {
            let mut success = store.contains(&cid).unwrap_or_default();
            if !success {
                let deadline = start.checked_add(timeout).expect("Infallible");
                success = self
                    .get_block_inner(&store, cid, deadline, validate_peer)
                    .await;
                // Spin check db when `get_block_inner` fails fast,
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

            if let Some(responder) = responder
                && let Err(e) = responder.send_async(success).await
            {
                debug!("{e}");
            }

            metrics::GET_BLOCK_TIME.observe((Instant::now() - start).as_secs_f64());
        });
    }

    async fn get_block_inner(
        &self,
        store: &(impl BitswapStoreReadWrite + ShallowClone),
        cid: Cid,
        deadline: Instant,
        validate_peer: Option<Arc<ValidatePeerCallback>>,
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

        let peers: Vec<_> = self.peers.read().iter().cloned().collect();
        let validated_peers: Vec<_> = peers
            .iter()
            .filter(|&&p| validate_peer.as_ref().map(|f| f(p)).unwrap_or(true))
            .cloned()
            .collect();

        debug!("Found {} valid peers for {cid}", validated_peers.len());
        let selected_peers = if validated_peers.is_empty() {
            // Fallback to all peers
            peers
        } else {
            validated_peers
        };

        for peer in selected_peers {
            if let Err(e) = self.outbound_have_request_tx.send((peer, cid)) {
                debug!("{e}");
            }
        }

        // Wait for the block off the blocking pool: react to `have` offers by
        // requesting the block, and take the first saved response, bounded by the
        // deadline. `have_open` stops polling the `have` channel once it closes
        // while still awaiting a saved response. `biased` keeps a saved response
        // that arrives right at the deadline from being dropped by a tie.
        let timeout = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline));
        tokio::pin!(timeout);
        let mut have_open = true;
        let response = loop {
            tokio::select! {
                biased;
                saved = block_saved_rx.recv_async() => break saved.ok(),
                () = &mut timeout => break None,
                have = block_have_rx.recv_async(), if have_open => match have {
                    Ok(peer) => {
                        _ = self.outbound_block_request_tx.send((peer, cid));
                    }
                    Err(_) => have_open = false,
                },
            }
        };

        let success = match response {
            // Block already in the db, nothing to insert.
            Some(None) => true,
            // Timed out or channel closed.
            None => false,
            // Inserting is blocking db IO with an unbounded tail (lock, commit,
            // compaction), so it stays off the async runtime.
            Some(Some(data)) => {
                let store = store.shallow_clone();
                task::spawn_blocking(move || match Block::new(cid, data) {
                    Ok(block) => match store.insert(&block) {
                        Ok(()) => {
                            metrics::message_counter_inbound_response_block_update_db().inc();
                            true
                        }
                        Err(e) => {
                            metrics::message_counter_inbound_response_block_update_db_failure()
                                .inc();
                            warn!(
                                "Failed to update db, cid: {cid}, data: {:?}, error: {e:#}",
                                block.data()
                            );
                            false
                        }
                    },
                    Err(e) => {
                        warn!("Failed to construct block, cid: {cid}, error: {e:#}");
                        false
                    }
                })
                .await
                .unwrap_or(false)
            }
        };

        // Cleanup
        {
            let mut response_channels = self.response_channels.write();
            if response_channels.remove(&cid).is_some() {
                response_channels.shrink_to_fit();
                metrics::response_channel_container_capacity()
                    .set(response_channels.total_capacity() as _);
            }
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
                            debug!("{e}");
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
            peers.shrink_to_fit();
            metrics::peer_container_capacity().set(peers.capacity() as _);
        }
        success
    }
}
