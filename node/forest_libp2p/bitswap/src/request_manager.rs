use crate::*;
use hashbrown::{HashMap, HashSet};
use libipld::Cid;
use libp2p::PeerId;

#[derive(Debug, Clone, Default)]
struct ResponseChannels {
    block_have: Vec<flume::Sender<PeerId>>,
    block_saved: Vec<flume::Sender<()>>,
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

    pub fn broadcast_have_request(
        &mut self,
        bitswap: &mut BitswapBehaviour,
        cid: Cid,
        response_tx: flume::Sender<PeerId>,
    ) {
        let have_request = BitswapRequest::new_have(cid).send_dont_have(false);
        for peer in &self.peers {
            bitswap.send_request(peer, have_request.clone());
        }
        self.response_channels
            .entry(cid)
            .or_insert(Default::default())
            .block_have
            .push(response_tx);
        metrics::response_channel_container_capacity().set(self.response_channels.capacity() as _);
    }

    pub fn send_block_request(
        &mut self,
        bitswap: &mut BitswapBehaviour,
        peer: &PeerId,
        cid: Cid,
        response_tx: flume::Sender<()>,
    ) {
        let block_request = BitswapRequest::new_block(cid).send_dont_have(true);
        bitswap.send_request(peer, block_request);
        self.response_channels
            .entry(cid)
            .or_insert(Default::default())
            .block_saved
            .push(response_tx);
        metrics::response_channel_container_capacity().set(self.response_channels.capacity() as _);
    }

    pub async fn on_inbound_response_event(&mut self, response: BitswapInboundResponseEvent) {
        use BitswapInboundResponseEvent::*;
        match response {
            HaveBlock(peer, cid) => {
                if let Some(chans) = self.response_channels.get(&cid) {
                    for tx in &chans.block_have {
                        _ = tx.send_async(peer).await;
                    }
                }
            }
            BlockSaved(_peer, cid) => {
                // Cleanup on success
                self.response_channels.remove(&cid);
                metrics::response_channel_container_capacity()
                    .set(self.response_channels.capacity() as _);
                if let Some(chans) = self.response_channels.get(&cid) {
                    for tx in &chans.block_saved {
                        _ = tx.send_async(()).await;
                    }
                }
            }
        }
    }
}
