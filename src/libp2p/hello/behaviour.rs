// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashMap;
use libp2p::{
    request_response::{self, ProtocolSupport, RequestId, ResponseChannel},
    swarm::{derive_prelude::*, NetworkBehaviour, THandlerOutEvent},
    PeerId,
};
use log::warn;

use super::*;
use crate::libp2p::service::metrics;

type InnerBehaviour = request_response::Behaviour<HelloCodec>;

pub struct HelloBehaviour {
    inner: InnerBehaviour,
    response_channels: HashMap<RequestId, flume::Sender<HelloResponse>>,
}

impl HelloBehaviour {
    pub fn send_request(
        &mut self,
        peer: &PeerId,
        request: HelloRequest,
        response_channel: flume::Sender<HelloResponse>,
    ) -> RequestId {
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

    pub async fn handle_response(&mut self, request_id: &RequestId, response: HelloResponse) {
        if let Some(channel) = self.response_channels.remove(request_id) {
            self.track_metrics();
            if let Err(err) = channel.send_async(response).await {
                warn!("{err}");
            }
        }
    }

    pub fn on_error(&mut self, request_id: &RequestId) {
        if self.response_channels.remove(request_id).is_some() {
            self.track_metrics();
        }
    }

    fn track_metrics(&self) {
        metrics::NETWORK_CONTAINER_CAPACITIES
            .with_label_values(&[metrics::values::HELLO_REQUEST_TABLE])
            .set(self.response_channels.capacity() as u64);
    }
}

impl Default for HelloBehaviour {
    fn default() -> Self {
        Self {
            inner: InnerBehaviour::new(
                HelloCodec::default(),
                [(HelloProtocolName, ProtocolSupport::Full)],
                Default::default(),
            ),
            response_channels: Default::default(),
        }
    }
}

impl NetworkBehaviour for HelloBehaviour {
    type ConnectionHandler = <InnerBehaviour as NetworkBehaviour>::ConnectionHandler;

    type OutEvent = <InnerBehaviour as NetworkBehaviour>::OutEvent;

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
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.inner
            .handle_established_outbound_connection(connection_id, peer, addr, role_override)
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

    fn on_swarm_event(&mut self, event: FromSwarm<Self::ConnectionHandler>) {
        self.inner.on_swarm_event(event)
    }

    fn poll(
        &mut self,
        cx: &mut std::task::Context<'_>,
        params: &mut impl PollParameters,
    ) -> std::task::Poll<ToSwarm<Self::OutEvent, THandlerInEvent<Self>>> {
        self.inner.poll(cx, params)
    }
}
