// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::rpc::{RPCEvent, RPCMessage, RPC};
use libp2p::core::identity::Keypair;
use libp2p::core::PeerId;
use libp2p::gossipsub::{Gossipsub, GossipsubConfig, GossipsubEvent, Topic, TopicHash};
use libp2p::identify::{Identify, IdentifyEvent};
use libp2p::mdns::{Mdns, MdnsEvent};
use libp2p::ping::{
    handler::{PingFailure, PingSuccess},
    Ping, PingEvent,
};
use libp2p::swarm::{NetworkBehaviourAction, NetworkBehaviourEventProcess, PollParameters};
use libp2p::NetworkBehaviour;
use log::debug;
use std::{task::Context, task::Poll};

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "ForestBehaviourEvent", poll_method = "poll")]
pub struct ForestBehaviour {
    pub gossipsub: Gossipsub,
    pub mdns: Mdns,
    pub ping: Ping,
    pub identify: Identify,
    pub rpc: RPC,
    #[behaviour(ignore)]
    events: Vec<ForestBehaviourEvent>,
}

#[derive(Debug)]
pub enum ForestBehaviourEvent {
    PeerDialed(PeerId),
    PeerDisconnected(PeerId),
    DiscoveredPeer(PeerId),
    ExpiredPeer(PeerId),
    GossipMessage {
        source: PeerId,
        topics: Vec<TopicHash>,
        message: Vec<u8>,
    },
    RPC(PeerId, RPCEvent),
}

impl NetworkBehaviourEventProcess<MdnsEvent> for ForestBehaviour {
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

impl NetworkBehaviourEventProcess<GossipsubEvent> for ForestBehaviour {
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

impl NetworkBehaviourEventProcess<PingEvent> for ForestBehaviour {
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

impl NetworkBehaviourEventProcess<IdentifyEvent> for ForestBehaviour {
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
impl NetworkBehaviourEventProcess<RPCMessage> for ForestBehaviour {
    fn inject_event(&mut self, event: RPCMessage) {
        match event {
            RPCMessage::PeerDialed(peer_id) => {
                self.events.push(ForestBehaviourEvent::PeerDialed(peer_id));
            }
            RPCMessage::PeerDisconnected(peer_id) => {
                self.events
                    .push(ForestBehaviourEvent::PeerDisconnected(peer_id));
            }
            RPCMessage::RPC(peer_id, rpc_event) => match rpc_event {
                RPCEvent::Request(req_id, request) => {
                    self.events.push(ForestBehaviourEvent::RPC(
                        peer_id,
                        RPCEvent::Request(req_id, request),
                    ));
                }
                RPCEvent::Response(req_id, response) => {
                    self.events.push(ForestBehaviourEvent::RPC(
                        peer_id,
                        RPCEvent::Response(req_id, response),
                    ));
                }
                RPCEvent::Error(req_id, err) => debug!("RPC Error {:?}, {:?}", err, req_id),
            },
        }
    }
}

impl ForestBehaviour {
    /// Consumes the events list when polled.
    fn poll<TBehaviourIn>(
        &mut self,
        _: &mut Context,
        _: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<TBehaviourIn, ForestBehaviourEvent>> {
        if !self.events.is_empty() {
            return Poll::Ready(NetworkBehaviourAction::GenerateEvent(self.events.remove(0)));
        }
        Poll::Pending
    }

    pub fn new(local_key: &Keypair) -> Self {
        let local_peer_id = local_key.public().into_peer_id();
        let gossipsub_config = GossipsubConfig::default();
        ForestBehaviour {
            gossipsub: Gossipsub::new(local_peer_id, gossipsub_config),
            mdns: Mdns::new().expect("Could not start mDNS"),
            ping: Ping::default(),
            identify: Identify::new("forest/libp2p".into(), "0.0.1".into(), local_key.public()),
            rpc: RPC::default(),
            events: vec![],
        }
    }

    /// Publish data over the gossip network.
    pub fn publish(&mut self, topic: &Topic, data: impl Into<Vec<u8>>) {
        self.gossipsub.publish(topic, data);
    }

    /// Subscribe to a gossip topic.
    pub fn subscribe(&mut self, topic: Topic) -> bool {
        self.gossipsub.subscribe(topic)
    }

    /// Send an RPC request or response to some peer.
    pub fn send_rpc(&mut self, peer_id: PeerId, req: RPCEvent) {
        self.rpc.send_rpc(peer_id, req);
    }
}
