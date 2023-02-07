// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashMap;
use libp2p::{
    request_response::{ProtocolSupport, RequestId, RequestResponse, ResponseChannel},
    swarm::NetworkBehaviour,
    PeerId,
};
use log::warn;

use super::*;
use crate::service::metrics;

type InnerBehaviour = RequestResponse<HelloCodec>;

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

    pub fn complete_request(&mut self, request_id: RequestId) {
        self.response_channels.remove(&request_id);
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
            inner: RequestResponse::new(
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
