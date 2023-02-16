// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use libp2p::{
    request_response::{
        ProtocolSupport, RequestId, RequestResponse, RequestResponseConfig, RequestResponseEvent,
    },
    swarm::NetworkBehaviour,
    PeerId,
};

use crate::{codec::*, protocol::*, request_manager::*, *};

/// `libp2p` swarm network behaviour event of `bitswap`
pub type BitswapBehaviourEvent = RequestResponseEvent<Vec<BitswapMessage>, ()>;

/// A `go-bitswap` compatible protocol that is built on top of
/// [RequestResponse].
pub struct BitswapBehaviour {
    inner: RequestResponse<BitswapRequestResponseCodec>,
    request_manager: Arc<BitswapRequestManager>,
}

impl BitswapBehaviour {
    /// Creates a [BitswapBehaviour] instance
    pub fn new(protocols: &[&'static [u8]], cfg: RequestResponseConfig) -> Self {
        assert!(!protocols.is_empty(), "protocols cannot be empty");

        let protocols: Vec<_> = protocols
            .iter()
            .map(|&n| (BitswapProtocol(n), ProtocolSupport::Full))
            .collect();
        BitswapBehaviour {
            inner: RequestResponse::new(BitswapRequestResponseCodec, protocols, cfg),
            request_manager: Default::default(),
        }
    }

    /// Gets mutable borrow of the inner [RequestResponse]
    pub fn inner_mut(&mut self) -> &mut RequestResponse<BitswapRequestResponseCodec> {
        &mut self.inner
    }

    /// Sends a [BitswapRequest] to a peer
    pub fn send_request(&mut self, peer: &PeerId, request: BitswapRequest) -> RequestId {
        match request.ty {
            RequestType::Have => metrics::message_counter_outbound_request_have().inc(),
            RequestType::Block => metrics::message_counter_outbound_request_block().inc(),
        }
        self.inner
            .send_request(peer, vec![BitswapMessage::Request(request)])
    }

    /// Sends a [BitswapResponse] to a peer
    pub fn send_response(&mut self, peer: &PeerId, response: (Cid, BitswapResponse)) -> RequestId {
        match response.1 {
            BitswapResponse::Have(..) => metrics::message_counter_outbound_response_have().inc(),
            BitswapResponse::Block(..) => metrics::message_counter_outbound_response_block().inc(),
        }
        self.inner
            .send_request(peer, vec![BitswapMessage::Response(response.0, response.1)])
    }
}

// Request Manager related API(s)
impl BitswapBehaviour {
    /// Gets the associated [BitswapRequestManager]
    pub fn request_manager(&self) -> Arc<BitswapRequestManager> {
        self.request_manager.clone()
    }

    /// Hook the `bitswap` network event into its [BitswapRequestManager]
    pub fn handle_event<S: BitswapStoreRead>(
        &mut self,
        store: &S,
        event: BitswapBehaviourEvent,
    ) -> anyhow::Result<()> {
        self.request_manager
            .clone()
            .handle_event(self, store, event)
    }
}

impl Default for BitswapBehaviour {
    fn default() -> Self {
        // This matches default values in `go-bitswap`
        BitswapBehaviour::new(
            &[
                b"/ipfs/bitswap/1.2.0",
                b"/ipfs/bitswap/1.1.0",
                b"/ipfs/bitswap/1.0.0",
                b"/ipfs/bitswap",
            ],
            Default::default(),
        )
    }
}

impl NetworkBehaviour for BitswapBehaviour {
    type ConnectionHandler =
        <RequestResponse<BitswapRequestResponseCodec> as NetworkBehaviour>::ConnectionHandler;

    type OutEvent = <RequestResponse<BitswapRequestResponseCodec> as NetworkBehaviour>::OutEvent;

    fn new_handler(&mut self) -> Self::ConnectionHandler {
        self.inner_mut().new_handler()
    }

    fn addresses_of_peer(&mut self, peer: &PeerId) -> Vec<libp2p::Multiaddr> {
        self.inner_mut().addresses_of_peer(peer)
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: libp2p::swarm::derive_prelude::ConnectionId,
        event: <<Self::ConnectionHandler as libp2p::swarm::IntoConnectionHandler>::Handler as
            libp2p::swarm::ConnectionHandler>::OutEvent,
    ) {
        self.inner_mut()
            .on_connection_handler_event(peer_id, connection_id, event)
    }

    fn on_swarm_event(
        &mut self,
        event: libp2p::swarm::derive_prelude::FromSwarm<Self::ConnectionHandler>,
    ) {
        match &event {
            libp2p::swarm::derive_prelude::FromSwarm::ConnectionEstablished(e) => {
                self.request_manager.on_peer_connected(e.peer_id);
            }
            libp2p::swarm::derive_prelude::FromSwarm::ConnectionClosed(e) => {
                self.request_manager.on_peer_disconnected(&e.peer_id);
            }
            _ => {}
        };

        self.inner_mut().on_swarm_event(event)
    }

    fn poll(
        &mut self,
        cx: &mut std::task::Context<'_>,
        params: &mut impl libp2p::swarm::PollParameters,
    ) -> std::task::Poll<
        libp2p::swarm::NetworkBehaviourAction<Self::OutEvent, Self::ConnectionHandler>,
    > {
        self.inner_mut().poll(cx, params)
    }
}
