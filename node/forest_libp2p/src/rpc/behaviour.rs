// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::handler::RPCHandler;
use super::RPCEvent;
use futures::task::Context;
use futures_util::task::Poll;
use libp2p::core::connection::ConnectionId;
use libp2p::swarm::{
    protocols_handler::ProtocolsHandler, NetworkBehaviour, NetworkBehaviourAction, NotifyHandler,
    PollParameters,
};
use libp2p::{Multiaddr, PeerId};

/// The RPC behaviour that gets consumed by the Swarm.
pub struct RPC {
    /// Queue of events to processed.
    events: Vec<NetworkBehaviourAction<RPCEvent, RPCMessage>>,
}

impl RPC {
    /// Creates a new RPC behaviour
    pub fn new() -> Self {
        RPC::default()
    }

    /// Send an RPCEvent to a peer specified by peer_id.
    pub fn send_rpc(&mut self, peer_id: PeerId, event: RPCEvent) {
        self.events.push(NetworkBehaviourAction::NotifyHandler {
            peer_id,
            event,
            handler: NotifyHandler::Any,
        });
    }
}

impl Default for RPC {
    fn default() -> Self {
        RPC { events: vec![] }
    }
}

/// Messages sent to the user from the RPC protocol.
#[derive(Debug)]
pub enum RPCMessage {
    RPC(PeerId, RPCEvent),
    PeerDialed(PeerId),
    PeerDisconnected(PeerId),
}

impl NetworkBehaviour for RPC {
    type ProtocolsHandler = RPCHandler;
    type OutEvent = RPCMessage;
    fn new_handler(&mut self) -> Self::ProtocolsHandler {
        RPCHandler::default()
    }

    fn addresses_of_peer(&mut self, _: &PeerId) -> Vec<Multiaddr> {
        vec![]
    }

    fn inject_connected(&mut self, peer_id: &PeerId) {
        self.events.push(NetworkBehaviourAction::GenerateEvent(
            RPCMessage::PeerDialed(peer_id.clone()),
        ));
    }

    fn inject_disconnected(&mut self, peer_id: &PeerId) {
        self.events.push(NetworkBehaviourAction::GenerateEvent(
            RPCMessage::PeerDisconnected(peer_id.clone()),
        ));
    }

    fn inject_event(
        &mut self,
        peer_id: PeerId,
        _connection: ConnectionId,
        event: <Self::ProtocolsHandler as ProtocolsHandler>::OutEvent,
    ) {
        self.events
            .push(NetworkBehaviourAction::GenerateEvent(RPCMessage::RPC(
                peer_id, event,
            )))
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
