// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    num::NonZeroUsize,
    sync::{Arc, LazyLock},
};

use ahash::HashMap;
use libp2p::{
    PeerId,
    request_response::{
        self, OutboundFailure, OutboundRequestId, ProtocolSupport, ResponseChannel,
    },
    swarm::{NetworkBehaviour, THandlerOutEvent, derive_prelude::*},
};
use nonzero_ext::nonzero;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tracing::debug;

use super::*;
use crate::{
    libp2p::{rpc::RequestResponseError, service::metrics},
    utils::misc::env::env_or_default_logged,
};

type InnerBehaviour = request_response::Behaviour<ChainExchangeCodec>;

/// Maximum number of concurrent inbound chain exchange requests Forest will
/// service. Excess requests are rejected with [`ChainExchangeResponseStatus::GoAway`].
static MAX_CONCURRENT_INBOUND_CHAIN_EXCHANGE_REQUESTS: LazyLock<NonZeroUsize> =
    LazyLock::new(|| {
        env_or_default_logged(
            "FOREST_MAX_CONCURRENT_INBOUND_CHAIN_EXCHANGE_REQUESTS",
            nonzero!(32_usize),
        )
    });

/// Per-peer cap on concurrent inbound chain exchange requests. Excess requests
/// from a single peer are rejected with [`ChainExchangeResponseStatus::GoAway`].
static MAX_CONCURRENT_INBOUND_CHAIN_EXCHANGE_REQUESTS_PER_PEER: LazyLock<NonZeroUsize> =
    LazyLock::new(|| {
        env_or_default_logged(
            "FOREST_MAX_CONCURRENT_INBOUND_CHAIN_EXCHANGE_REQUESTS_PER_PEER",
            nonzero!(4_usize),
        )
    });

pub struct ChainExchangeBehaviour {
    inner: InnerBehaviour,
    response_channels: HashMap<
        OutboundRequestId,
        flume::Sender<Result<ChainExchangeResponse, RequestResponseError>>,
    >,
    request_limiter: Arc<Semaphore>,
    per_peer_limiters: HashMap<PeerId, Arc<Semaphore>>,
}

impl ChainExchangeBehaviour {
    pub fn new(cfg: request_response::Config) -> Self {
        Self {
            inner: InnerBehaviour::new(
                [(CHAIN_EXCHANGE_PROTOCOL_NAME, ProtocolSupport::Full)],
                cfg,
            ),
            response_channels: Default::default(),
            request_limiter: Arc::new(Semaphore::new(
                MAX_CONCURRENT_INBOUND_CHAIN_EXCHANGE_REQUESTS.get(),
            )),
            per_peer_limiters: Default::default(),
        }
    }

    pub fn try_acquire_request_permit(&self) -> Option<OwnedSemaphorePermit> {
        self.request_limiter.clone().try_acquire_owned().ok()
    }

    /// Lazily creates a per-peer semaphore on first request from `peer`.
    pub fn try_acquire_peer_permit(&mut self, peer: PeerId) -> Option<OwnedSemaphorePermit> {
        self.per_peer_limiters
            .entry(peer)
            .or_insert_with(|| {
                Arc::new(Semaphore::new(
                    MAX_CONCURRENT_INBOUND_CHAIN_EXCHANGE_REQUESTS_PER_PEER.get(),
                ))
            })
            .clone()
            .try_acquire_owned()
            .ok()
    }

    fn on_peer_connection_closed(&mut self, peer: PeerId, remaining_established: usize) {
        if remaining_established == 0 {
            self.per_peer_limiters.remove(&peer);
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
        if let Some(tx) = self.response_channels.remove(request_id)
            && let Err(err) = tx.send(Err(error.into()))
        {
            // Demoting log level here because the same request might be sent to multiple
            // remote peers simultaneously, it's expected that outbound failures that happen
            // after receiving the first successful response could be sent to a closed
            // channel.
            debug!("{err}");
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
        if let FromSwarm::ConnectionClosed(c) = &event {
            self.on_peer_connection_closed(c.peer_id, c.remaining_established);
        }
        self.inner.on_swarm_event(event)
    }

    fn poll(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        self.inner.poll(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_behaviour() -> ChainExchangeBehaviour {
        ChainExchangeBehaviour::new(request_response::Config::default())
    }

    #[test]
    fn per_peer_limiter_saturates_independently() {
        let mut behaviour = new_behaviour();
        let peer_a = PeerId::random();
        let peer_b = PeerId::random();
        let cap = MAX_CONCURRENT_INBOUND_CHAIN_EXCHANGE_REQUESTS_PER_PEER.get();

        let mut permits_a = Vec::new();
        for _ in 0..cap {
            permits_a.push(
                behaviour
                    .try_acquire_peer_permit(peer_a)
                    .expect("peer_a should have permits available"),
            );
        }
        assert!(
            behaviour.try_acquire_peer_permit(peer_a).is_none(),
            "peer_a should be saturated at its per-peer cap",
        );
        assert!(
            behaviour.try_acquire_peer_permit(peer_b).is_some(),
            "peer_b should not be affected by peer_a's saturation",
        );

        permits_a.clear();
        assert!(
            behaviour.try_acquire_peer_permit(peer_a).is_some(),
            "peer_a should be acquirable after permits are dropped",
        );
    }

    #[test]
    fn global_limiter_saturates() {
        let behaviour = new_behaviour();
        let cap = MAX_CONCURRENT_INBOUND_CHAIN_EXCHANGE_REQUESTS.get();

        let permits: Vec<_> = (0..cap)
            .map(|_| {
                behaviour
                    .try_acquire_request_permit()
                    .expect("global cap not yet reached")
            })
            .collect();
        assert!(
            behaviour.try_acquire_request_permit().is_none(),
            "global limiter should be saturated",
        );
        drop(permits);
        assert!(
            behaviour.try_acquire_request_permit().is_some(),
            "global limiter should release permits when dropped",
        );
    }

    #[test]
    fn per_peer_entry_removed_on_full_disconnect() {
        let mut behaviour = new_behaviour();
        let peer_a = PeerId::random();
        let _permit = behaviour.try_acquire_peer_permit(peer_a);
        assert!(behaviour.per_peer_limiters.contains_key(&peer_a));

        behaviour.on_peer_connection_closed(peer_a, 1);
        assert!(
            behaviour.per_peer_limiters.contains_key(&peer_a),
            "entry should be retained while other connections remain",
        );

        behaviour.on_peer_connection_closed(peer_a, 0);
        assert!(
            !behaviour.per_peer_limiters.contains_key(&peer_a),
            "entry should be removed when last connection closes",
        );
    }
}
