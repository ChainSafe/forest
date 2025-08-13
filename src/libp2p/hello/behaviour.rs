// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    sync::Arc,
    task::Poll,
    time::{Duration, Instant},
};

use ahash::HashMap;
use libp2p::{
    PeerId,
    request_response::{self, OutboundRequestId, ProtocolSupport, ResponseChannel},
    swarm::{CloseConnection, NetworkBehaviour, THandlerOutEvent, derive_prelude::*},
};
use tracing::warn;

use super::*;
use crate::libp2p::{PeerManager, service::metrics};

type InnerBehaviour = request_response::Behaviour<HelloCodec>;

pub struct HelloBehaviour {
    inner: InnerBehaviour,
    response_channels: HashMap<OutboundRequestId, flume::Sender<HelloResponse>>,
    pending_inbound_hello_peers: HashMap<PeerId, Instant>,
    peer_manager: Arc<PeerManager>,
}

impl HelloBehaviour {
    pub fn new(cfg: request_response::Config, peer_manager: Arc<PeerManager>) -> Self {
        Self {
            inner: InnerBehaviour::new([(HELLO_PROTOCOL_NAME, ProtocolSupport::Full)], cfg),
            response_channels: Default::default(),
            pending_inbound_hello_peers: Default::default(),
            peer_manager,
        }
    }

    pub fn send_request(
        &mut self,
        peer: &PeerId,
        request: HelloRequest,
        response_channel: flume::Sender<HelloResponse>,
    ) -> OutboundRequestId {
        let request_id = self.inner.send_request(peer, request);
        self.response_channels.insert(request_id, response_channel);
        self.track_metrics();
        request_id
    }

    pub fn send_response(
        &mut self,
        channel: ResponseChannel<HelloResponse>,
        response: HelloResponse,
    ) -> Result<(), HelloResponse> {
        self.inner.send_response(channel, response)
    }

    pub async fn handle_response(
        &mut self,
        request_id: &OutboundRequestId,
        response: HelloResponse,
    ) {
        if let Some(channel) = self.response_channels.remove(request_id) {
            self.track_metrics();
            if let Err(err) = channel.send_async(response).await {
                warn!("{err}");
            }
        }
    }

    pub fn on_outbound_failure(&mut self, request_id: &OutboundRequestId) {
        if self.response_channels.remove(request_id).is_some() {
            self.track_metrics();
        }
    }

    fn track_metrics(&self) {
        metrics::NETWORK_CONTAINER_CAPACITIES
            .get_or_create(&metrics::values::HELLO_REQUEST_TABLE)
            .set(self.response_channels.capacity() as _);
    }
}

impl NetworkBehaviour for HelloBehaviour {
    type ConnectionHandler = <InnerBehaviour as NetworkBehaviour>::ConnectionHandler;

    type ToSwarm = <InnerBehaviour as NetworkBehaviour>::ToSwarm;

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &libp2p::Multiaddr,
        remote_addr: &libp2p::Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.inner.handle_established_inbound_connection(
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
        addr: &Multiaddr,
        role_override: libp2p::core::Endpoint,
        port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.inner.handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            role_override,
            port_use,
        )
    }

    fn handle_pending_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        local_addr: &libp2p::Multiaddr,
        remote_addr: &libp2p::Multiaddr,
    ) -> Result<(), ConnectionDenied> {
        self.inner
            .handle_pending_inbound_connection(connection_id, local_addr, remote_addr)
    }

    fn handle_pending_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        addresses: &[libp2p::Multiaddr],
        effective_role: libp2p::core::Endpoint,
    ) -> Result<Vec<libp2p::Multiaddr>, ConnectionDenied> {
        self.inner.handle_pending_outbound_connection(
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
        self.inner
            .on_connection_handler_event(peer_id, connection_id, event)
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        if let FromSwarm::ConnectionEstablished(e) = &event
            && e.other_established == 0
        {
            self.pending_inbound_hello_peers
                .insert(e.peer_id, Instant::now());
        }

        self.inner.on_swarm_event(event)
    }

    fn poll(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Poll::Ready(ev) = self.inner.poll(cx) {
            // Remove a peer from `pending_inbound_hello_peers` when its hello request is received.
            if let ToSwarm::GenerateEvent(request_response::Event::Message {
                peer,
                message:
                    request_response::Message::Request {
                        request: HelloRequest { .. },
                        ..
                    },
                ..
            }) = &ev
            {
                self.pending_inbound_hello_peers.remove(peer);
            }

            return Poll::Ready(ev);
        }

        // Disconnect peers whose hello request are not received after a TIMEOUT
        const INBOUND_HELLO_WAIT_TIMEOUT: Duration = Duration::from_secs(30);
        let now = Instant::now();
        if let Some((&peer_to_disconnect, _)) =
            self.pending_inbound_hello_peers
                .iter()
                .find(|&(_, &connected_instant)| {
                    now.duration_since(connected_instant) > INBOUND_HELLO_WAIT_TIMEOUT
                })
        {
            self.pending_inbound_hello_peers.remove(&peer_to_disconnect);
            if !self.peer_manager.is_peer_protected(&peer_to_disconnect) {
                tracing::debug!(peer=%peer_to_disconnect, "Disconnecting peer for not receiving hello in 30s");
                return Poll::Ready(ToSwarm::CloseConnection {
                    peer_id: peer_to_disconnect,
                    connection: CloseConnection::All,
                });
            }
        }

        Poll::Pending
    }
}
