use libp2p::core::ConnectedPoint;
use libp2p::swarm::{
    protocols_handler::ProtocolsHandler, NetworkBehaviour, NetworkBehaviourAction, PollParameters,
    SubstreamProtocol,
};
use futures::task::Context;
use futures_util::task::Poll;
use libp2p::{PeerId, Multiaddr};
use super::handler::RPCHandler;
use super::RPCEvent;

use std::marker::PhantomData;
use futures::prelude::*;

pub struct RPC<TSubstream> {
    /// Queue of events to processed.
    /// TODO: This isn't correct
    events: Vec<NetworkBehaviourAction<RPCEvent, RPCEvent>>,
    /// Pins the generic substream.
    marker: PhantomData<(TSubstream)>,
}

impl<TSubstream> RPC<TSubstream> {
    pub fn new () -> Self {
        RPC {
            events: Vec::new(),
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
       // RPCHandler::new()
        unimplemented!()
    }

    fn addresses_of_peer(&mut self, peer_id: &PeerId) -> Vec<Multiaddr> {
        unimplemented!()
    }

    fn inject_connected(&mut self, peer_id: PeerId, endpoint: ConnectedPoint) {
        unimplemented!()
    }

    fn inject_disconnected(&mut self, peer_id: &PeerId, endpoint: ConnectedPoint) {
        unimplemented!()
    }

    fn inject_node_event(&mut self, peer_id: PeerId, event: <Self::ProtocolsHandler as ProtocolsHandler>::OutEvent) {
        unimplemented!()
    }

    fn poll(&mut self, cx: &mut Context, params: &mut impl PollParameters) -> Poll<NetworkBehaviourAction<<Self::ProtocolsHandler as ProtocolsHandler>::InEvent, Self::OutEvent>> {
        unimplemented!()
    }
}