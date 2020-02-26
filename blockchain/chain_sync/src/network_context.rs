// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::sync::Sender;
use blocks::TipSetKeys;
use forest_libp2p::{
    rpc::{BlockSyncRequest, RPCEvent, RPCRequest, RequestId},
    NetworkMessage,
};
use libp2p::core::PeerId;
use log::trace;

/// Context used in chain sync to handle sequential
pub struct SyncNetworkContext {
    /// Channel to send network messages through p2p service
    network_send: Sender<NetworkMessage>,

    /// Handles sequential request ID enumeration for requests
    request_id: RequestId,
}

// #[async_trait]
// impl BlockSyncProvider for SyncNetworkContext {
//     async fn get_headers(&mut self, _tsk: &TipSetKeys, _count: u64) -> Result<Vec<Tipset>, String> {
//         self.blocksync_request(peer_id: PeerId, request: Message)
//         Ok(vec![])
//     }
// }

impl SyncNetworkContext {
    pub fn new(network_send: Sender<NetworkMessage>) -> Self {
        Self {
            network_send,
            request_id: 0,
        }
    }

    pub async fn blocksync_headers(
        &mut self,
        peer_id: PeerId,
        tsk: &TipSetKeys,
        count: u64,
    ) -> RequestId {
        self.blocksync_request(
            peer_id,
            BlockSyncRequest {
                start: tsk.cids().to_vec(),
                request_len: count,
                options: 1,
            },
        )
        .await
    }

    /// Send a blocksync request to the network
    pub async fn blocksync_request(
        &mut self,
        peer_id: PeerId,
        request: BlockSyncRequest,
    ) -> RequestId {
        trace!("Sending Blocksync Request {:?}", request);
        self.send_rpc_request(peer_id, RPCRequest::Blocksync(request))
            .await
    }

    /// Send any RPC request to the network
    pub async fn send_rpc_request(
        &mut self,
        peer_id: PeerId,
        rpc_request: RPCRequest,
    ) -> RequestId {
        let request_id = self.request_id;
        self.request_id += 1;
        self.send_rpc_event(peer_id, RPCEvent::Request(request_id, rpc_request))
            .await;
        request_id
    }

    /// Handles sending the base event to the network service
    async fn send_rpc_event(&mut self, peer_id: PeerId, event: RPCEvent) {
        self.network_send
            .send(NetworkMessage::RPC {
                peer_id,
                // TODO probably change event name to event from request
                request: event,
            })
            .await
    }
}
