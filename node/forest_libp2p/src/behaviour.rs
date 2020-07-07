// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocksync::{
    BlockSyncCodec, BlockSyncProtocolName, BlockSyncRequest, BlockSyncResponse,
};
use crate::config::Libp2pConfig;
use crate::hello::{HelloCodec, HelloProtocolName, HelloRequest, HelloResponse};
use crate::rpc::RPCRequest;
use libp2p::core::identity::Keypair;
use libp2p::core::PeerId;
use libp2p::gossipsub::{Gossipsub, GossipsubConfig, GossipsubEvent, Topic, TopicHash};
use libp2p::identify::{Identify, IdentifyEvent};
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{Kademlia, KademliaConfig, KademliaEvent, QueryId};
use libp2p::mdns::{Mdns, MdnsEvent};
use libp2p::multiaddr::Protocol;
use libp2p::ping::{
    handler::{PingFailure, PingSuccess},
    Ping, PingEvent,
};
use libp2p::swarm::{NetworkBehaviourAction, NetworkBehaviourEventProcess, PollParameters};
use libp2p::NetworkBehaviour;
use libp2p_request_response::{
    ProtocolSupport, RequestId, RequestResponse, RequestResponseEvent, RequestResponseMessage,
    ResponseChannel,
};
use log::{debug, trace, warn};
use std::collections::HashSet;
use std::{task::Context, task::Poll};

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "ForestBehaviourEvent", poll_method = "poll")]
pub struct ForestBehaviour {
    gossipsub: Gossipsub,
    // TODO configure to allow turning mdns off
    mdns: Mdns,
    ping: Ping,
    identify: Identify,
    hello: RequestResponse<HelloCodec>,
    blocksync: RequestResponse<BlockSyncCodec>,
    kademlia: Kademlia<MemoryStore>,
    #[behaviour(ignore)]
    events: Vec<ForestBehaviourEvent>,
    #[behaviour(ignore)]
    peers: HashSet<PeerId>,
}

#[derive(Debug)]
pub enum ForestBehaviourEvent {
    PeerDialed(PeerId),
    PeerDisconnected(PeerId),
    GossipMessage {
        source: PeerId,
        topics: Vec<TopicHash>,
        message: Vec<u8>,
    },
    HelloRequest {
        peer: PeerId,
        request: HelloRequest,
        channel: ResponseChannel<HelloResponse>,
    },
    HelloResponse {
        peer: PeerId,
        request_id: RequestId,
        response: HelloResponse,
    },
    BlockSyncRequest {
        peer: PeerId,
        request: BlockSyncRequest,
        channel: ResponseChannel<BlockSyncResponse>,
    },
    BlockSyncResponse {
        peer: PeerId,
        request_id: RequestId,
        response: BlockSyncResponse,
    },
}

impl NetworkBehaviourEventProcess<MdnsEvent> for ForestBehaviour {
    fn inject_event(&mut self, event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(list) => {
                for (peer, _) in list {
                    trace!("mdns: Discovered peer {}", peer.to_base58());
                    self.add_peer(peer);
                }
            }
            MdnsEvent::Expired(list) => {
                for (peer, _) in list {
                    if !self.mdns.has_node(&peer) {
                        self.remove_peer(&peer);
                    }
                }
            }
        }
    }
}

impl NetworkBehaviourEventProcess<KademliaEvent> for ForestBehaviour {
    fn inject_event(&mut self, event: KademliaEvent) {
        match event {
            KademliaEvent::RoutingUpdated { peer, .. } => {
                self.add_peer(peer);
            }
            event => {
                trace!("kad: {:?}", event);
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
                trace!(
                    "PingSuccess::Ping rtt to {} is {} ms",
                    event.peer.to_base58(),
                    rtt.as_millis()
                );
            }
            Result::Ok(PingSuccess::Pong) => {
                trace!("PingSuccess::Pong from {}", event.peer.to_base58());
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
                trace!("Identified Peer {}", peer_id);
                trace!("protocol_version {}", info.protocol_version);
                trace!("agent_version {}", info.agent_version);
                trace!("listening_ addresses {:?}", info.listen_addrs);
                trace!("observed_address {}", observed_addr);
                trace!("protocols {:?}", info.protocols);
            }
            IdentifyEvent::Sent { .. } => (),
            IdentifyEvent::Error { .. } => (),
        }
    }
}

impl NetworkBehaviourEventProcess<RequestResponseEvent<HelloRequest, HelloResponse>>
    for ForestBehaviour
{
    fn inject_event(&mut self, event: RequestResponseEvent<HelloRequest, HelloResponse>) {
        match event {
            RequestResponseEvent::Message { peer, message } => match message {
                RequestResponseMessage::Request { request, channel } => {
                    self.events.push(ForestBehaviourEvent::HelloRequest {
                        peer,
                        request,
                        channel,
                    })
                }
                RequestResponseMessage::Response {
                    request_id,
                    response,
                } => self.events.push(ForestBehaviourEvent::HelloResponse {
                    peer,
                    request_id,
                    response,
                }),
            },
            RequestResponseEvent::OutboundFailure {
                peer,
                request_id,
                error,
            } => warn!(
                "Hello outbound failure (peer: {:?}) (id: {:?}): {:?}",
                peer, request_id, error
            ),
            RequestResponseEvent::InboundFailure { peer, error } => {
                warn!("Hello inbound error (peer: {:?}): {:?}", peer, error)
            }
        }
    }
}

impl NetworkBehaviourEventProcess<RequestResponseEvent<BlockSyncRequest, BlockSyncResponse>>
    for ForestBehaviour
{
    fn inject_event(&mut self, event: RequestResponseEvent<BlockSyncRequest, BlockSyncResponse>) {
        match event {
            RequestResponseEvent::Message { peer, message } => match message {
                RequestResponseMessage::Request { request, channel } => {
                    self.events.push(ForestBehaviourEvent::BlockSyncRequest {
                        peer,
                        request,
                        channel,
                    })
                }
                RequestResponseMessage::Response {
                    request_id,
                    response,
                } => self.events.push(ForestBehaviourEvent::BlockSyncResponse {
                    peer,
                    request_id,
                    response,
                }),
            },
            RequestResponseEvent::OutboundFailure {
                peer,
                request_id,
                error,
            } => warn!(
                "BlockSync outbound error (peer: {:?}) (id: {:?}): {:?}",
                peer, request_id, error
            ),
            RequestResponseEvent::InboundFailure { peer, error } => {
                warn!("BlockSync onbound error (peer: {:?}): {:?}", peer, error)
            }
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

    pub fn new(local_key: &Keypair, config: &Libp2pConfig, network_name: &str) -> Self {
        let local_peer_id = local_key.public().into_peer_id();
        let gossipsub_config = GossipsubConfig::default();

        // Kademlia config
        let store = MemoryStore::new(local_peer_id.to_owned());
        let mut kad_config = KademliaConfig::default();
        let network = format!("/fil/kad/{}/kad/1.0.0", network_name);
        kad_config.set_protocol_name(network.as_bytes().to_vec());
        let mut kademlia = Kademlia::with_config(local_peer_id.to_owned(), store, kad_config);
        for multiaddr in config.bootstrap_peers.iter() {
            let mut addr = multiaddr.to_owned();
            if let Some(Protocol::P2p(mh)) = addr.pop() {
                let peer_id = PeerId::from_multihash(mh).unwrap();
                kademlia.add_address(&peer_id, addr);
            } else {
                warn!("Could not add addr {} to Kademlia DHT", multiaddr)
            }
        }
        if let Err(e) = kademlia.bootstrap() {
            warn!("Kademlia bootstrap failed: {}", e);
        }

        let hp = std::iter::once((HelloProtocolName, ProtocolSupport::Full));
        let bp = std::iter::once((BlockSyncProtocolName, ProtocolSupport::Full));

        ForestBehaviour {
            gossipsub: Gossipsub::new(local_peer_id, gossipsub_config),
            mdns: Mdns::new().expect("Could not start mDNS"),
            ping: Ping::default(),
            identify: Identify::new(
                "ipfs/0.1.0".into(),
                // TODO update to include actual version
                format!("forest-{}", "0.1.0"),
                local_key.public(),
            ),
            kademlia,
            hello: RequestResponse::new(HelloCodec, hp, Default::default()),
            blocksync: RequestResponse::new(BlockSyncCodec, bp, Default::default()),
            events: vec![],
            peers: Default::default(),
        }
    }

    /// Bootstrap Kademlia network
    pub fn bootstrap(&mut self) -> Result<QueryId, String> {
        self.kademlia.bootstrap().map_err(|e| e.to_string())
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
    pub fn send_rpc_request(&mut self, peer_id: &PeerId, req: RPCRequest, id: RequestId) {
        match req {
            RPCRequest::Hello(request) => self.hello.send_request_with_id(peer_id, request, id),
            RPCRequest::BlockSync(request) => {
                self.blocksync.send_request_with_id(peer_id, request, id)
            }
        }
    }

    /// Adds peer to the peer set.
    pub fn add_peer(&mut self, peer_id: PeerId) {
        self.peers.insert(peer_id);
    }

    /// Adds peer to the peer set.
    pub fn remove_peer(&mut self, peer_id: &PeerId) {
        self.peers.remove(peer_id);
    }

    /// Adds peer to the peer set.
    pub fn peers(&self) -> &HashSet<PeerId> {
        &self.peers
    }
}
