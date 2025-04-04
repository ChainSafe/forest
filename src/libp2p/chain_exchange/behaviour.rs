// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashMap;
use libp2p::{
    PeerId,
    request_response::{
        self, OutboundFailure, OutboundRequestId, ProtocolSupport, ResponseChannel,
    },
    swarm::{NetworkBehaviour, THandlerOutEvent, derive_prelude::*},
};
use tracing::debug;

use super::*;
use crate::libp2p::{rpc::RequestResponseError, service::metrics};

type InnerBehaviour = request_response::Behaviour<ChainExchangeCodec>;

pub struct ChainExchangeBehaviour {
    inner: InnerBehaviour,
    response_channels: HashMap<
        OutboundRequestId,
        flume::Sender<Result<ChainExchangeResponse, RequestResponseError>>,
    >,
}

impl ChainExchangeBehaviour {
    pub fn new(cfg: request_response::Config) -> Self {
        Self {
            inner: InnerBehaviour::new(
                [(CHAIN_EXCHANGE_PROTOCOL_NAME, ProtocolSupport::Full)],
                cfg,
            ),
            response_channels: Default::default(),
        }
    }

    pub fn send_request(
        &mut self,
        peer: &PeerId,
        request: ChainExchangeRequest,
        response_channel: flume::Sender<Result<ChainExchangeResponse, RequestResponseError>>,
    ) -> OutboundRequestId {
        let request_id = self.inner.send_request(peer, request);
        self.response_channels.insert(request_id, response_channel);
        self.track_metrics();
        request_id
    }

    pub fn send_response(
        &mut self,
        channel: ResponseChannel<ChainExchangeResponse>,
        response: ChainExchangeResponse,
    ) -> Result<(), ChainExchangeResponse> {
        self.inner.send_response(channel, response)
    }

    pub async fn handle_inbound_response(
        &mut self,
        request_id: &OutboundRequestId,
        response: ChainExchangeResponse,
    ) {
        if let Some(channel) = self.response_channels.remove(request_id) {
            self.track_metrics();
            if let Err(err) = channel.send_async(Ok(response)).await {
                // Demoting log level here because the same request might be sent to multiple
                // remote peers simultaneously, it's expected that responses that arrive late
                // might be sent to a closed channel
                debug!("{err}");
            }
        }
    }

    pub fn on_outbound_error(&mut self, request_id: &OutboundRequestId, error: OutboundFailure) {
        self.track_metrics();
        if let Some(tx) = self.response_channels.remove(request_id) {
            if let Err(err) = tx.send(Err(error.into())) {
                // Demoting log level here because the same request might be sent to multiple
                // remote peers simultaneously, it's expected that outbound failures that happen
                // after receiving the first successful response could be sent to a closed
                // channel.
                debug!("{err}");
            }
        }
    }

    fn track_metrics(&self) {
        metrics::NETWORK_CONTAINER_CAPACITIES
            .get_or_create(&metrics::values::CHAIN_EXCHANGE_REQUEST_TABLE)
            .set(self.response_channels.capacity() as _);
    }
}

impl NetworkBehaviour for ChainExchangeBehaviour {
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
        addr: &libp2p::Multiaddr,
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
        self.inner.on_swarm_event(event)
    }

    fn poll(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        self.inner.poll(cx)
    }
}
