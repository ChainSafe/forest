// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use libp2p::{
    request_response::{self, ProtocolSupport, RequestId},
    swarm::{derive_prelude::*, NetworkBehaviour, THandlerOutEvent},
    PeerId,
};

use crate::libp2p_bitswap::{codec::*, request_manager::*, *};

/// `libp2p` swarm network behavior event of `bitswap`
pub type BitswapBehaviourEvent = request_response::Event<Vec<BitswapMessage>, ()>;

/// A `go-bitswap` compatible protocol that is built on top of
/// [`request_response::Behaviour`].
pub struct BitswapBehaviour {
    inner: request_response::Behaviour<BitswapRequestResponseCodec>,
    request_manager: Arc<BitswapRequestManager>,
}

impl BitswapBehaviour {
    /// Creates a [`BitswapBehaviour`] instance
    pub fn new(protocols: &[&'static str], cfg: request_response::Config) -> Self {
        assert!(!protocols.is_empty(), "protocols cannot be empty");

        let protocols: Vec<_> = protocols
            .iter()
            .map(|&n| (n, ProtocolSupport::Full))
            .collect();
        BitswapBehaviour {
            inner: request_response::Behaviour::new(protocols, cfg),
            request_manager: Default::default(),
        }
    }

    /// Gets mutable borrow of the inner [`request_response::Behaviour`]
    pub fn inner_mut(&mut self) -> &mut request_response::Behaviour<BitswapRequestResponseCodec> {
        &mut self.inner
    }

    /// Sends a [`BitswapRequest`] to a peer
    pub fn send_request(&mut self, peer: &PeerId, request: BitswapRequest) -> RequestId {
        if request.cancel {
            metrics::message_counter_outbound_request_cancel().inc();
        } else {
            match request.ty {
                RequestType::Have => metrics::message_counter_outbound_request_have().inc(),
                RequestType::Block => metrics::message_counter_outbound_request_block().inc(),
            }
        }
        self.inner
            .send_request(peer, vec![BitswapMessage::Request(request)])
    }

    /// Sends a [`BitswapResponse`] to a peer
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
    /// Gets the associated [`BitswapRequestManager`]
    pub fn request_manager(&self) -> Arc<BitswapRequestManager> {
        self.request_manager.clone()
    }

    /// Hook the `bitswap` network event into its [`BitswapRequestManager`]
    #[cfg(test)]
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
                "/ipfs/bitswap/1.2.0",
                "/ipfs/bitswap/1.1.0",
                "/ipfs/bitswap/1.0.0",
                "/ipfs/bitswap",
            ],
            Default::default(),
        )
    }
}

impl NetworkBehaviour for BitswapBehaviour {
    type ConnectionHandler =
        <request_response::Behaviour<BitswapRequestResponseCodec> as NetworkBehaviour>::ConnectionHandler;

    type ToSwarm =
        <request_response::Behaviour<BitswapRequestResponseCodec> as NetworkBehaviour>::ToSwarm;

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &libp2p::Multiaddr,
        remote_addr: &libp2p::Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.inner_mut().handle_established_inbound_connection(
            connection_id,
            peer,
            local_addr,
            remote_addr,
        )
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        addr: &libp2p::Multiaddr,
        role_override: libp2p::core::Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.inner_mut().handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            role_override,
        )
    }

    fn handle_pending_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        local_addr: &libp2p::Multiaddr,
        remote_addr: &libp2p::Multiaddr,
    ) -> Result<(), ConnectionDenied> {
        self.inner_mut()
            .handle_pending_inbound_connection(connection_id, local_addr, remote_addr)
    }

    fn handle_pending_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        addresses: &[libp2p::Multiaddr],
        effective_role: libp2p::core::Endpoint,
    ) -> Result<Vec<libp2p::Multiaddr>, ConnectionDenied> {
        self.inner_mut().handle_pending_outbound_connection(
            connection_id,
            maybe_peer,
            addresses,
            effective_role,
        )
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        self.inner_mut()
            .on_connection_handler_event(peer_id, connection_id, event)
    }

    fn on_swarm_event(&mut self, event: FromSwarm<Self::ConnectionHandler>) {
        match &event {
            FromSwarm::ConnectionEstablished(e) => {
                self.request_manager.on_peer_connected(e.peer_id);
            }
            FromSwarm::ConnectionClosed(e) => {
                self.request_manager.on_peer_disconnected(&e.peer_id);
            }
            _ => {}
        };

        self.inner_mut().on_swarm_event(event)
    }

    fn poll(
        &mut self,
        cx: &mut std::task::Context<'_>,
        params: &mut impl PollParameters,
    ) -> std::task::Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        self.inner_mut().poll(cx, params)
    }
}
