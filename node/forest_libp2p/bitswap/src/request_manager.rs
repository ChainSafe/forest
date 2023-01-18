use crate::*;
use hashbrown::{HashMap, HashSet};
use libipld::Cid;
use libp2p::PeerId;
use parking_lot::RwLock;
use std::{sync::Arc, time::Duration};

#[derive(Debug, Clone)]
struct ResponseChannels {
    // block_have: flume::Sender<PeerId>,
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

    pub fn get_block(self: Arc<Self>, cid: Cid, timeout: Duration, responder: flume::Sender<bool>) {
        tokio::spawn(async move {
            if let Err(e) = tokio::task::spawn_blocking(move || {
                let success = self.get_block_sync(cid, timeout);
                if let Err(e) = responder.send(success) {
                    warn!("{e}");
                }
            })
            .await
            {
                warn!("{e}");
            }
        });
    }

    pub fn get_block_sync(&self, cid: Cid, timeout: Duration) -> bool {
        info!("get_block_sync start");
        if self.response_channels.read().contains_key(&cid) {
            // TODO: spin and check db
            return false;
        }

        let (block_saved_tx, block_saved_rx) = flume::unbounded();
        let channels = ResponseChannels {
            block_saved: block_saved_tx,
        };
        {
            self.response_channels.write().insert(cid, channels);
        }

        let block_request = BitswapRequest::new_block(cid).send_dont_have(false);
        for &peer in self.peers.read().iter() {
            if let Err(e) = self.outbound_request_tx.send((peer, block_request.clone())) {
                warn!("{e}");
            }
        }

        info!("get_block_sync waiting for block_saved_rx");
        let success = block_saved_rx.recv_timeout(timeout).is_ok();
        info!("get_block_sync waiting for block_saved_rx, done: {success}");

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
                // if let Some(chans) = self.response_channels.get(&cid) {
                //     info!("on_inbound_response_event: have sending");
                //     join_all(chans.block_have.iter().map(|tx| tx.send_async(peer))).await;
                //     info!("on_inbound_response_event: have sent");
                // }
            }
            BlockSaved(_peer, cid) => {
                if let Some(chans) = self.response_channels.read().get(&cid) {
                    info!("on_inbound_response_event: block sending");
                    _ = chans.block_saved.send(());
                    info!("on_inbound_response_event: block sent");
                }
            }
        }
    }
}
