// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::handler::GraphSyncHandler;
use crate::GraphSyncMessage;
use futures::task::Context;
use futures_util::task::Poll;
use libp2p::core::connection::ConnectionId;
use libp2p::swarm::{
    protocols_handler::ProtocolsHandler, NetworkBehaviour, NetworkBehaviourAction, PollParameters,
};
use libp2p::{Multiaddr, PeerId};

/// The RPC behaviour that gets consumed by the Swarm.
pub struct RPC {
    /// Queue of events to processed.
    events: Vec<NetworkBehaviourAction<GraphSyncMessage, GraphSyncEvent>>,
}

impl RPC {
    /// Creates a new RPC behaviour
    pub fn new() -> Self {
        RPC::default()
    }
}

impl Default for RPC {
    fn default() -> Self {
        RPC { events: vec![] }
    }
}

impl NetworkBehaviour for RPC {
    type ProtocolsHandler = GraphSyncHandler;
    type OutEvent = GraphSyncEvent;
    fn new_handler(&mut self) -> Self::ProtocolsHandler {
        GraphSyncHandler::default()
    }

    fn addresses_of_peer(&mut self, _: &PeerId) -> Vec<Multiaddr> {
        vec![]
    }

    fn inject_connected(&mut self, _peer_id: &PeerId) {
        todo!()
    }

    fn inject_disconnected(&mut self, _peer_id: &PeerId) {
        todo!()
    }

    fn inject_event(
        &mut self,
        _peer_id: PeerId,
        _connection: ConnectionId,
        _event: <Self::ProtocolsHandler as ProtocolsHandler>::OutEvent,
    ) {
        todo!()
    }

    fn poll(
        &mut self,
        _: &mut Context,
        _: &mut impl PollParameters,
    ) -> Poll<
        NetworkBehaviourAction<
            <Self::ProtocolsHandler as ProtocolsHandler>::InEvent,
            Self::OutEvent,
        >,
    > {
        if !self.events.is_empty() {
            return Poll::Ready(self.events.remove(0));
        }
        Poll::Pending
    }
}

// TODO remove
#[allow(dead_code)]
/// Event from the GraphSync behaviour.
#[derive(Debug)]
pub enum GraphSyncEvent {
    /// A message has been received. This contains the PeerId that we received the message from
    /// and the message itself.
    Message(PeerId, GraphSyncMessage),

    Connected(PeerId),
    Disconnected(PeerId),
}
