use super::protocol::{BlockSyncConfig, Message};

use libp2p::core::ConnectedPoint;
use libp2p::swarm::protocols_handler::{DummyProtocolsHandler, OneShotHandler, ProtocolsHandler};
use libp2p::swarm::{NetworkBehaviour, NetworkBehaviourAction, PollParameters};
use libp2p::{Multiaddr, PeerId};

use std::collections::VecDeque;
use std::marker::PhantomData;
use tokio::prelude::*;
use futures::{AsyncWrite, AsyncRead};
use std::task::Context;

pub struct BlockSync<TSubstream> {
    marker: PhantomData<TSubstream>,
    events: VecDeque<NetworkBehaviourAction<Message, ()>>,
    connected_peers: Vec<PeerId>,
}

impl<TSubstream> BlockSync<TSubstream> {
    pub fn new() -> Self {
        BlockSync {
            marker: PhantomData,
            events: VecDeque::new(),
            connected_peers: Vec::new(),
        }
    }

    pub fn send_want_list(&mut self) {
        let peer_id = self.connected_peers[0].clone();
        let message: Message = Message {
            start: vec![],
            request_len: 0,
            options: 0,
        };
        self.events.push_back(NetworkBehaviourAction::SendEvent {
            peer_id: peer_id.clone(),
            event: message,
        });
    }
}

#[derive(Debug)]
pub enum BlockSyncEvent {
    /// We received a `Message` from a remote.
    Rx(Message),
    /// We successfully sent a `Message`.
    Tx,
}

impl From<Message> for BlockSyncEvent {
    #[inline]
    fn from(message: Message) -> BlockSyncEvent {
        BlockSyncEvent::Rx(message)
    }
}

impl From<()> for BlockSyncEvent {
    #[inline]
    fn from(_: ()) -> BlockSyncEvent {
        BlockSyncEvent::Tx
    }
}

impl<TSubstream> NetworkBehaviour for BlockSync<TSubstream>
where
    TSubstream: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type ProtocolsHandler = OneShotHandler<TSubstream, BlockSyncConfig, Message, BlockSyncEvent>;
    type OutEvent = ();

    fn new_handler(&mut self) -> Self::ProtocolsHandler {
        Default::default()
    }

    fn addresses_of_peer(&mut self, peer_id: &PeerId) -> Vec<Multiaddr> {
        self.connected_peers.clone()
    }

    fn inject_connected(&mut self, peer_id: PeerId, endpoint: ConnectedPoint) {
        println!("Adding {:?}", peer_id);
        self.connected_peers.push(peer_id);
    }

    fn inject_disconnected(&mut self, peer_id: &PeerId, endpoint: ConnectedPoint) {}

    fn inject_node_event(&mut self, peer_id: PeerId, event: BlockSyncEvent) {
        println!("received event {:?}", event);

        let message = match event {
            BlockSyncEvent::Rx(message) => message,
            BlockSyncEvent::Tx => {
                return;
            }
        };
    }

    fn poll(
        &mut self,
        _: Context,
        _: &mut impl PollParameters,
    ) -> Async<
        NetworkBehaviourAction<
            <Self::ProtocolsHandler as ProtocolsHandler>::InEvent,
            Self::OutEvent,
        >,
    > {
        if let Some(event) = self.events.pop_front() {
            return Async::Ready(event);
        }
        Async::NotReady
    }
}
