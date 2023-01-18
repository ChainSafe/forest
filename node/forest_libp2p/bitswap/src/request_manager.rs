use crate::*;
use hashbrown::{HashMap, HashSet};
use libipld::Cid;
use libp2p::PeerId;
use std::time::Duration;

#[derive(Debug, Clone)]
struct ResponseChannels {
    // block_have: flume::Sender<PeerId>,
    block_saved: flume::Sender<()>,
}

// TODO: Use message queue like `go-bitswap`
#[derive(Debug, Default)]
pub struct BitswapRequestManager {
    peers: HashSet<PeerId>,
    response_channels: HashMap<Cid, ResponseChannels>,
}

impl BitswapRequestManager {
    pub fn add_peer(&mut self, peer: PeerId) -> bool {
        let r = self.peers.insert(peer);
        if r {
            metrics::peer_container_capacity().set(self.peers.capacity() as _);
        }
        r
    }

    pub fn remove_peer(&mut self, peer: &PeerId) -> bool {
        let r = self.peers.remove(peer);
        if r {
            metrics::peer_container_capacity().set(self.peers.capacity() as _);
        }
        r
    }

    pub async fn get_block_sync(
        &mut self,
        bitswap: &mut BitswapBehaviour,
        cid: Cid,
        timeout: Duration,
    ) -> bool {
        info!("get_block_sync start");
        let (block_saved_tx, block_saved_rx) = flume::unbounded();
        let channels = ResponseChannels {
            block_saved: block_saved_tx,
        };

        self.response_channels.insert(cid, channels);
        let block_request = BitswapRequest::new_block(cid).send_dont_have(false);
        for peer in &self.peers {
            bitswap.send_request(peer, block_request.clone());
        }

        info!("get_block_sync waiting for block_saved_rx");
        let success = tokio::task::spawn_blocking(move || block_saved_rx.recv_timeout(timeout))
            .await
            .is_ok();
        info!("get_block_sync waiting for block_saved_rx, done: {success}");

        self.response_channels.remove(&cid);

        metrics::response_channel_container_capacity().set(self.response_channels.capacity() as _);

        success
    }

    pub async fn on_inbound_response_event(&mut self, response: BitswapInboundResponseEvent) {
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
                // info!("on_inbound_response_event: block");
                if let Some(chans) = self.response_channels.get(&cid) {
                    info!("on_inbound_response_event: block sending");
                    _ = chans.block_saved.send_async(()).await;
                    info!("on_inbound_response_event: block sent");
                }
            }
        }
    }
}
