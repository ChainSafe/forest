use crate::*;
use flume::TryRecvError;
use hashbrown::{HashMap, HashSet};
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
    pub fn add_peer(&self, peer: PeerId) -> bool {
        let r = self.peers.write().insert(peer);
        if r {
            metrics::peer_container_capacity().set(self.peers.read().capacity() as _);
        }
        r
    }

    pub fn remove_peer(&self, peer: &PeerId) -> bool {
        let r = self.peers.write().remove(peer);
        if r {
            metrics::peer_container_capacity().set(self.peers.read().capacity() as _);
        }
        r
    }

    pub fn get_block(
        self: Arc<Self>,
        store: Arc<impl BitswapStore>,
        cid: Cid,
        timeout: Duration,
        responder: flume::Sender<bool>,
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

            if let Err(e) = responder.send_async(success).await {
                warn!("{e}");
            }

            timer.observe_duration();
        });
    }

    fn get_block_sync(&self, cid: Cid, deadline: Instant) -> bool {
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
            self.response_channels.write().remove(&cid);
            metrics::response_channel_container_capacity()
                .set(self.response_channels.read().capacity() as _);
        }

        success
    }

    pub async fn on_inbound_response_event(&self, response: BitswapInboundResponseEvent) {
        use BitswapInboundResponseEvent::*;
        match response {
            HaveBlock(peer, cid) => {
                // info!("on_inbound_response_event: have");
                if let Some(chans) = self.response_channels.read().get(&cid) {
                    _ = chans.block_have.send(peer);
                }
            }
            BlockSaved(_peer, cid) => {
                if let Some(chans) = self.response_channels.read().get(&cid) {
                    _ = chans.block_saved.send(());
                }
            }
        }
    }
}
