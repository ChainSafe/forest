// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::handler::RPCHandler;
use super::RPCEvent;
use futures::prelude::*;
use futures::task::Context;
use futures_util::task::Poll;
use libp2p::core::ConnectedPoint;
use libp2p::swarm::{
    protocols_handler::ProtocolsHandler, NetworkBehaviour, NetworkBehaviourAction, PollParameters,
};
use libp2p::{Multiaddr, PeerId};
use std::marker::PhantomData;

pub struct RPC<TSubstream> {
    /// Queue of events to processed.
    /// TODO: This isn't correct
    events: Vec<NetworkBehaviourAction<RPCEvent, RPCEvent>>,
    /// Pins the generic substream.
    marker: PhantomData<TSubstream>,
}

impl<TSubstream> RPC<TSubstream> {
    pub fn new() -> Self {
        RPC::default()
    }

    pub fn send_rpc(&self, payload: RPCEvent) {}
}

impl<TSubstream> Default for RPC<TSubstream> {
    fn default() -> Self {
        RPC {
            events: vec![],
            marker: PhantomData,
        }
    }
}

impl<TSubstream> NetworkBehaviour for RPC<TSubstream>
where
    TSubstream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    type ProtocolsHandler = RPCHandler<TSubstream>;
    type OutEvent = RPCEvent;
    fn new_handler(&mut self) -> Self::ProtocolsHandler {
        RPCHandler::default()
    }

    fn addresses_of_peer(&mut self, _: &PeerId) -> Vec<Multiaddr> {
        vec![]
    }

    fn inject_connected(&mut self, _: PeerId, _: ConnectedPoint) {
        // Dont need to impl this
    }

    fn inject_disconnected(&mut self, _: &PeerId, _: ConnectedPoint) {
        // Dont need to impl this
    }

    fn inject_node_event(
        &mut self,
        _: PeerId,
        event: <Self::ProtocolsHandler as ProtocolsHandler>::OutEvent,
    ) {
        self.events
            .push(NetworkBehaviourAction::GenerateEvent(event))
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
