// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashMap;
use libp2p::{
    request_response::{self, OutboundFailure, ProtocolSupport, RequestId, ResponseChannel},
    swarm::{derive_prelude::*, NetworkBehaviour, THandlerOutEvent},
    PeerId,
};
use tracing::debug;

use super::*;
use crate::libp2p::{rpc::RequestResponseError, service::metrics};

type InnerBehaviour = request_response::Behaviour<ChainExchangeCodec>;

pub struct ChainExchangeBehaviour {
    inner: InnerBehaviour,
    response_channels:
        HashMap<RequestId, flume::Sender<Result<ChainExchangeResponse, RequestResponseError>>>,
}

impl ChainExchangeBehaviour {
    pub fn send_request(
        &mut self,
        peer: &PeerId,
        request: ChainExchangeRequest,
        response_channel: flume::Sender<Result<ChainExchangeResponse, RequestResponseError>>,
    ) -> RequestId {
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
        request_id: &RequestId,
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

    pub fn on_outbound_error(&mut self, request_id: &RequestId, error: OutboundFailure) {
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
            .with_label_values(&[metrics::values::CHAIN_EXCHANGE_REQUEST_TABLE])
            .set(self.response_channels.capacity() as u64);
    }
}

impl Default for ChainExchangeBehaviour {
    fn default() -> Self {
        Self {
            inner: InnerBehaviour::new(
                ChainExchangeCodec::default(),
                [(ChainExchangeProtocolName, ProtocolSupport::Full)],
                Default::default(),
            ),
            response_channels: Default::default(),
        }
    }
}

impl NetworkBehaviour for ChainExchangeBehaviour {
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
        addr: &libp2p::Multiaddr,
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
