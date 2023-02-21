// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashMap;
use libp2p::{
    request_response::{
        OutboundFailure, ProtocolSupport, RequestId, RequestResponse, ResponseChannel,
    },
    swarm::NetworkBehaviour,
    PeerId,
};
use log::warn;

use super::*;
use crate::{rpc::RequestResponseError, service::metrics};

type InnerBehaviour = RequestResponse<ChainExchangeCodec>;

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

    pub fn complete_request(&mut self, request_id: RequestId) {
        self.response_channels.remove(&request_id);
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
                warn!("{err}");
            }
        }
    }

    pub fn on_outbound_error(&mut self, request_id: &RequestId, error: OutboundFailure) {
        self.track_metrics();
        if let Some(tx) = self.response_channels.remove(request_id) {
            if let Err(err) = tx.send(Err(error.into())) {
                warn!("{err}");
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
            inner: RequestResponse::new(
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

    fn new_handler(&mut self) -> Self::ConnectionHandler {
        self.inner.new_handler()
    }

    fn addresses_of_peer(&mut self, peer: &PeerId) -> Vec<libp2p::Multiaddr> {
        self.inner.addresses_of_peer(peer)
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: libp2p::swarm::derive_prelude::ConnectionId,
        event: <<Self::ConnectionHandler as libp2p::swarm::IntoConnectionHandler>::Handler as
            libp2p::swarm::ConnectionHandler>::OutEvent,
    ) {
        self.inner
            .on_connection_handler_event(peer_id, connection_id, event)
    }

    fn on_swarm_event(
        &mut self,
        event: libp2p::swarm::derive_prelude::FromSwarm<Self::ConnectionHandler>,
    ) {
        self.inner.on_swarm_event(event)
    }

    fn poll(
        &mut self,
        cx: &mut std::task::Context<'_>,
        params: &mut impl libp2p::swarm::PollParameters,
    ) -> std::task::Poll<
        libp2p::swarm::NetworkBehaviourAction<Self::OutEvent, Self::ConnectionHandler>,
    > {
        self.inner.poll(cx, params)
    }
}
