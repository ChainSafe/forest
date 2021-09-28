// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::config::GraphSyncConfig;
use super::handler::GraphSyncHandler;
use crate::{Extensions, GraphSyncMessage};
use cid::Cid;
use forest_ipld::selector::Selector;
use futures::task::Context;
use futures_util::task::Poll;
use libp2p::core::connection::ConnectionId;
use libp2p::swarm::{
    protocols_handler::ProtocolsHandler, NetworkBehaviour, NetworkBehaviourAction, NotifyHandler,
    PollParameters,
};
use libp2p::{Multiaddr, PeerId};
use log::debug;
use std::collections::{HashSet, VecDeque};

/// The GraphSync behaviour that gets consumed by the Swarm.
#[derive(Default)]
pub struct GraphSync {
    /// Config options for the service
    config: GraphSyncConfig,

    /// Queue of events to processed.
    events: VecDeque<NetworkBehaviourAction<GraphSyncMessage, ()>>,

    // TODO just temporary, will probably have to attach some data with peers
    peers: HashSet<PeerId>,
}

impl GraphSync {
    /// Creates a new GraphSync behaviour
    pub fn new(config: GraphSyncConfig) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }

    /// Initiates GraphSync request to peer given root and selector.
    pub fn send_request(
        &mut self,
        _peer_id: PeerId,
        _root: Cid,
        _selector: Selector,
        _extensions: Extensions,
    ) {
        todo!()
    }
}

impl NetworkBehaviour for GraphSync {
    type ProtocolsHandler = GraphSyncHandler;
    // TODO this will need to be updated to include data emitted from the GS responses
    type OutEvent = ();

    fn new_handler(&mut self) -> Self::ProtocolsHandler {
        GraphSyncHandler::new(
            self.config.protocol_id.clone(),
            self.config.max_transmit_size,
        )
    }

    fn addresses_of_peer(&mut self, _: &PeerId) -> Vec<Multiaddr> {
        Vec::new()
    }

    fn inject_connected(&mut self, peer_id: &PeerId) {
        debug!("New peer connected: {:?}", peer_id);
        self.peers.insert(peer_id.clone());
    }

    fn inject_disconnected(&mut self, peer_id: &PeerId) {
        debug!("Peer disconnected: {:?}", peer_id);
        self.peers.remove(peer_id);
    }

    fn inject_event(
        &mut self,
        peer_id: PeerId,
        _connection: ConnectionId,
        event: GraphSyncMessage,
    ) {
        self.events
            .push_back(NetworkBehaviourAction::NotifyHandler {
                peer_id,
                event,
                handler: NotifyHandler::Any,
            });
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
        if let Some(event) = self.events.pop_front() {
            return Poll::Ready(event);
        }
        Poll::Pending
    }
}
