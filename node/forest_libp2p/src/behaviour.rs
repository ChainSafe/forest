// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use futures::prelude::*;
use libp2p::core::identity::Keypair;
use libp2p::core::PeerId;
use libp2p::gossipsub::{Gossipsub, GossipsubConfig, GossipsubEvent, Topic, TopicHash};
use libp2p::identify::{Identify, IdentifyEvent};
use libp2p::mdns::{Mdns, MdnsEvent};
use libp2p::ping::{
    handler::{PingFailure, PingSuccess},
    Ping, PingEvent,
};
use libp2p::swarm::{NetworkBehaviourAction, NetworkBehaviourEventProcess};
use libp2p::NetworkBehaviour;
use log::debug;
use std::{task::Context, task::Poll};

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "ForestBehaviourEvent", poll_method = "poll")]
pub struct ForestBehaviour<TSubstream: AsyncRead + AsyncWrite + Unpin + Send + 'static> {
    pub gossipsub: Gossipsub<TSubstream>,
    pub mdns: Mdns<TSubstream>,
    pub ping: Ping<TSubstream>,
    pub identify: Identify<TSubstream>,
    #[behaviour(ignore)]
    events: Vec<ForestBehaviourEvent>,
}

#[derive(Debug)]
pub enum ForestBehaviourEvent {
    DiscoveredPeer(PeerId),
    ExpiredPeer(PeerId),
    GossipMessage {
        source: PeerId,
        topics: Vec<TopicHash>,
        message: Vec<u8>,
    },
}

impl<TSubstream: AsyncRead + AsyncWrite + Unpin + Send + 'static>
    NetworkBehaviourEventProcess<MdnsEvent> for ForestBehaviour<TSubstream>
{
    fn inject_event(&mut self, event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(list) => {
                for (peer, _) in list {
                    self.events.push(ForestBehaviourEvent::DiscoveredPeer(peer))
                }
            }
            MdnsEvent::Expired(list) => {
                for (peer, _) in list {
                    if !self.mdns.has_node(&peer) {
                        self.events.push(ForestBehaviourEvent::ExpiredPeer(peer))
                    }
                }
            }
        }
    }
}

impl<TSubstream: AsyncRead + AsyncWrite + Unpin + Send + 'static>
    NetworkBehaviourEventProcess<GossipsubEvent> for ForestBehaviour<TSubstream>
{
    fn inject_event(&mut self, message: GossipsubEvent) {
        if let GossipsubEvent::Message(_, _, message) = message {
            self.events.push(ForestBehaviourEvent::GossipMessage {
                source: message.source,
                topics: message.topics,
                message: message.data,
            })
        }
    }
}

impl<TSubstream: AsyncRead + AsyncWrite + Unpin + Send + 'static>
    NetworkBehaviourEventProcess<PingEvent> for ForestBehaviour<TSubstream>
{
    fn inject_event(&mut self, event: PingEvent) {
        match event.result {
            Result::Ok(PingSuccess::Ping { rtt }) => {
                debug!(
                    "PingSuccess::Ping rtt to {} is {} ms",
                    event.peer.to_base58(),
                    rtt.as_millis()
                );
            }
            Result::Ok(PingSuccess::Pong) => {
                debug!("PingSuccess::Pong from {}", event.peer.to_base58());
            }
            Result::Err(PingFailure::Timeout) => {
                debug!("PingFailure::Timeout {}", event.peer.to_base58());
            }
            Result::Err(PingFailure::Other { error }) => {
                debug!("PingFailure::Other {}: {}", event.peer.to_base58(), error);
            }
        }
    }
}

impl<TSubstream: AsyncRead + AsyncWrite + Unpin + Send + 'static>
    NetworkBehaviourEventProcess<IdentifyEvent> for ForestBehaviour<TSubstream>
{
    fn inject_event(&mut self, event: IdentifyEvent) {
        match event {
            IdentifyEvent::Received {
                peer_id,
                info,
                observed_addr,
            } => {
                debug!("Identified Peer {:?}", peer_id);
                debug!("protocol_version {:}?", info.protocol_version);
                debug!("agent_version {:?}", info.agent_version);
                debug!("listening_ addresses {:?}", info.listen_addrs);
                debug!("observed_address {:?}", observed_addr);
                debug!("protocols {:?}", info.protocols);
            }
            IdentifyEvent::Sent { .. } => (),
            IdentifyEvent::Error { .. } => (),
        }
    }
}

impl<TSubstream: AsyncRead + AsyncWrite + Send + Unpin + 'static> ForestBehaviour<TSubstream> {
    /// Consumes the events list when polled.
    fn poll<TBehaviourIn>(
        &mut self,
        _: &mut Context,
    ) -> Poll<NetworkBehaviourAction<TBehaviourIn, ForestBehaviourEvent>> {
        if !self.events.is_empty() {
            return Poll::Ready(NetworkBehaviourAction::GenerateEvent(self.events.remove(0)));
        }
        Poll::Pending
    }
}

impl<TSubstream: AsyncRead + AsyncWrite + Unpin + Send + 'static> ForestBehaviour<TSubstream> {
    pub fn new(local_key: &Keypair) -> Self {
        let local_peer_id = local_key.public().into_peer_id();
        let gossipsub_config = GossipsubConfig::default();
        ForestBehaviour {
            gossipsub: Gossipsub::new(local_peer_id, gossipsub_config),
            mdns: Mdns::new().expect("Could not start mDNS"),
            ping: Ping::default(),
            identify: Identify::new("forest/libp2p".into(), "0.0.1".into(), local_key.public()),
            events: vec![],
        }
    }

    pub fn publish(&mut self, topic: &Topic, data: impl Into<Vec<u8>>) {
        self.gossipsub.publish(topic, data);
    }

    pub fn subscribe(&mut self, topic: Topic) -> bool {
        self.gossipsub.subscribe(topic)
    }
}
