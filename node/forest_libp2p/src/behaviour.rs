// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::config::Libp2pConfig;
use crate::hello::{HelloCodec, HelloProtocolName, HelloRequest, HelloResponse};
use crate::{
    chain_exchange::{
        ChainExchangeCodec, ChainExchangeProtocolName, ChainExchangeRequest, ChainExchangeResponse,
    },
    rpc::RequestResponseError,
};
use forest_cid::Cid;
use futures::channel::oneshot::{self, Sender as OneShotSender};
use futures::{prelude::*, stream::FuturesUnordered};
use libp2p::core::identity::Keypair;
use libp2p::core::PeerId;
use libp2p::gossipsub::{
    error::PublishError, Gossipsub, GossipsubConfig, GossipsubEvent, MessageAuthenticity, Topic,
    TopicHash, ValidationMode,
};
use libp2p::identify::{Identify, IdentifyEvent};
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::{Kademlia, KademliaConfig, KademliaEvent, QueryId};
use libp2p::mdns::{Mdns, MdnsEvent};
use libp2p::multiaddr::Protocol;
use libp2p::ping::{
    handler::{PingFailure, PingSuccess},
    Ping, PingEvent,
};
use libp2p::request_response::{
    ProtocolSupport, RequestId, RequestResponse, RequestResponseConfig, RequestResponseEvent,
    RequestResponseMessage, ResponseChannel,
};
use libp2p::swarm::{
    toggle::Toggle, NetworkBehaviourAction, NetworkBehaviourEventProcess, PollParameters,
};
use libp2p::NetworkBehaviour;
use libp2p_bitswap::{Bitswap, BitswapEvent, Priority};
use log::{debug, trace, warn};
use std::collections::HashMap;
use std::collections::HashSet;
use std::convert::TryFrom;
use std::error::Error;
use std::pin::Pin;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{task::Context, task::Poll};
use tiny_cid::Cid as Cid2;

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "ForestBehaviourEvent", poll_method = "poll")]
pub struct ForestBehaviour {
    gossipsub: Gossipsub,
    mdns: Toggle<Mdns>,
    ping: Ping,
    identify: Identify,
    // TODO would be nice to have this handled together and generic, to avoid duplicated polling
    // but is fine for now, since the protocols are handled slightly differently.
    hello: RequestResponse<HelloCodec>,
    chain_exchange: RequestResponse<ChainExchangeCodec>,
    kademlia: Toggle<Kademlia<MemoryStore>>,
    bitswap: Bitswap,
    #[behaviour(ignore)]
    events: Vec<ForestBehaviourEvent>,
    #[behaviour(ignore)]
    peers: HashSet<PeerId>,
    /// Keeps track of Chain exchange requests to responses
    #[behaviour(ignore)]
    cx_request_table:
        HashMap<RequestId, OneShotSender<Result<ChainExchangeResponse, RequestResponseError>>>,
    /// Boxed futures of responses for Chain Exchange incoming requests. This needs to be polled
    /// in the behaviour to have access to the `RequestResponse` protocol when sending response.
    ///
    /// This technically shouldn't be necessary, because the response can just be sent through the
    /// internal channel, but is necessary to avoid forking `RequestResponse`.
    #[behaviour(ignore)]
    cx_pending_responses:
        FuturesUnordered<Pin<Box<dyn Future<Output = Option<RequestProcessingOutcome>> + Send>>>,
}

struct RequestProcessingOutcome {
    inner_channel: ResponseChannel<ChainExchangeResponse>,
    response: ChainExchangeResponse,
}

#[derive(Debug)]
pub enum ForestBehaviourEvent {
    PeerDialed(PeerId),
    PeerDisconnected(PeerId),
    GossipMessage {
        source: Option<PeerId>,
        topics: Vec<TopicHash>,
        message: Vec<u8>,
    },
    BitswapReceivedBlock(PeerId, Cid, Box<[u8]>),
    BitswapReceivedWant(PeerId, Cid),
    HelloRequest {
        peer: PeerId,
        request: HelloRequest,
    },
    HelloResponse {
        peer: PeerId,
        request_id: RequestId,
        response: HelloResponse,
    },
    ChainExchangeRequest {
        peer: PeerId,
        request: ChainExchangeRequest,
        channel: OneShotSender<ChainExchangeResponse>,
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
                if self.mdns.is_enabled() {
                    for (peer, _) in list {
                        if !self.mdns.as_ref().unwrap().has_node(&peer) {
                            self.remove_peer(&peer);
                        }
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

impl NetworkBehaviourEventProcess<BitswapEvent> for ForestBehaviour {
    fn inject_event(&mut self, event: BitswapEvent) {
        match event {
            BitswapEvent::ReceivedBlock(peer_id, cid, data) => {
                // The `cid` from this event has a different type
                let cid = cid.to_bytes();
                match Cid::try_from(cid) {
                    Ok(cid) => self.events.push(ForestBehaviourEvent::BitswapReceivedBlock(
                        peer_id, cid, data,
                    )),
                    Err(e) => warn!("Fail to convert Cid: {}", e.to_string()),
                }
            }
            BitswapEvent::ReceivedWant(peer_id, cid, _priority) => {
                // The `cid` from this event has a different type
                let cid = cid.to_bytes();
                match Cid::try_from(cid) {
                    Ok(cid) => self
                        .events
                        .push(ForestBehaviourEvent::BitswapReceivedWant(peer_id, cid)),
                    Err(e) => warn!("Fail to convert Cid: {}", e.to_string()),
                }
            }
            BitswapEvent::ReceivedCancel(_peer_id, _cid) => {
                // TODO: Determine how to handle cancel
                trace!("BitswapEvent::ReceivedCancel, unimplemented");
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
                RequestResponseMessage::Request {
                    request,
                    channel,
                    request_id: _,
                } => {
                    let arrival = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .expect("System time before unix epoch")
                        .as_nanos();

                    debug!("Received hello request: {:?}", request);
                    let sent = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .expect("System time before unix epoch")
                        .as_nanos();

                    // Send hello response immediately, no need to have the overhead of emitting
                    // channel and polling future here.
                    self.hello
                        .send_response(channel, HelloResponse { arrival, sent });
                    self.events
                        .push(ForestBehaviourEvent::HelloRequest { request, peer });
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
            RequestResponseEvent::InboundFailure {
                peer,
                error,
                request_id: _,
            } => {
                warn!("Hello inbound error (peer: {:?}): {:?}", peer, error)
            }
        }
    }
}

impl NetworkBehaviourEventProcess<RequestResponseEvent<ChainExchangeRequest, ChainExchangeResponse>>
    for ForestBehaviour
{
    fn inject_event(
        &mut self,
        event: RequestResponseEvent<ChainExchangeRequest, ChainExchangeResponse>,
    ) {
        match event {
            RequestResponseEvent::Message { peer, message } => match message {
                RequestResponseMessage::Request {
                    request,
                    channel,
                    request_id: _,
                } => {
                    // This creates a new channel to be used to handle the response from
                    // outside the libp2p service. This is necessary because libp2p req-res does
                    // not expose the response channel and this is better to control the interface.
                    let (tx, rx) = oneshot::channel();
                    self.cx_pending_responses.push(Box::pin(async move {
                        // Await on created channel response
                        rx.await
                            .map(|response| RequestProcessingOutcome {
                                inner_channel: channel,
                                response,
                            })
                            .ok()
                    }));

                    self.events
                        .push(ForestBehaviourEvent::ChainExchangeRequest {
                            peer,
                            request,
                            // Sender for the channel polled above will be emitted and handled
                            // by whatever consumes the events
                            channel: tx,
                        })
                }
                RequestResponseMessage::Response {
                    request_id,
                    response,
                } => {
                    let tx = self.cx_request_table.remove(&request_id);

                    // Send the sucessful response through channel out.
                    if let Some(tx) = tx {
                        if tx.send(Ok(response)).is_err() {
                            debug!("RPCResponse receive timed out")
                        }
                    } else {
                        debug!("RPCResponse receive failed: channel not found");
                    };
                }
            },
            RequestResponseEvent::OutboundFailure {
                peer,
                request_id,
                error,
            } => {
                warn!(
                    "ChainExchange outbound error (peer: {:?}) (id: {:?}): {:?}",
                    peer, request_id, error
                );

                let tx = self.cx_request_table.remove(&request_id);

                // Send error through channel out.
                if let Some(tx) = tx {
                    if tx.send(Err(error.into())).is_err() {
                        debug!("RPCResponse receive failed")
                    }
                }
            }
            RequestResponseEvent::InboundFailure {
                peer,
                error,
                request_id: _,
            } => warn!(
                "ChainExchange inbound error (peer: {:?}): {:?}",
                peer, error
            ),
        }
    }
}

impl ForestBehaviour {
    /// Consumes the events list when polled.
    fn poll<TBehaviourIn>(
        &mut self,
        cx: &mut Context,
        _: &mut impl PollParameters,
    ) -> Poll<NetworkBehaviourAction<TBehaviourIn, ForestBehaviourEvent>> {
        // Poll to see if any response is ready to be sent back.
        while let Poll::Ready(Some(outcome)) = self.cx_pending_responses.poll_next_unpin(cx) {
            let RequestProcessingOutcome {
                inner_channel,
                response,
            } = match outcome {
                Some(outcome) => outcome,
                // The response builder was too busy and thus the request was dropped. This is
                // later on reported as a `InboundFailure::Omission`.
                None => continue,
            };

            self.chain_exchange.send_response(inner_channel, response)
        }
        if !self.events.is_empty() {
            return Poll::Ready(NetworkBehaviourAction::GenerateEvent(self.events.remove(0)));
        }
        Poll::Pending
    }

    pub fn new(local_key: &Keypair, config: &Libp2pConfig, network_name: &str) -> Self {
        let local_peer_id = local_key.public().into_peer_id();
        let gossipsub_config = GossipsubConfig {
            validation_mode: ValidationMode::Strict,
            // Using go gossipsub default, not certain this is intended
            max_transmit_size: 1 << 20,
            ..Default::default()
        };

        let mut bitswap = Bitswap::new();

        // Kademlia config
        let store = MemoryStore::new(local_peer_id.to_owned());
        let mut kad_config = KademliaConfig::default();
        let network = format!("/fil/kad/{}/kad/1.0.0", network_name);
        kad_config.set_protocol_name(network.as_bytes().to_vec());
        let kademlia_opt = if config.kademlia {
            let mut kademlia = Kademlia::with_config(local_peer_id, store, kad_config);
            for multiaddr in config.bootstrap_peers.iter() {
                let mut addr = multiaddr.to_owned();
                if let Some(Protocol::P2p(mh)) = addr.pop() {
                    let peer_id = PeerId::from_multihash(mh).unwrap();
                    kademlia.add_address(&peer_id, addr);
                    bitswap.connect(peer_id);
                } else {
                    warn!("Could not add addr {} to Kademlia DHT", multiaddr)
                }
            }
            if let Err(e) = kademlia.bootstrap() {
                warn!("Kademlia bootstrap failed: {}", e);
            }
            Some(kademlia)
        } else {
            None
        };

        let mdns_opt = if config.mdns {
            Some(Mdns::new().expect("Could not start mDNS"))
        } else {
            None
        };

        let hp = std::iter::once((HelloProtocolName, ProtocolSupport::Full));
        let cp = std::iter::once((ChainExchangeProtocolName, ProtocolSupport::Full));

        let mut req_res_config = RequestResponseConfig::default();
        req_res_config.set_request_timeout(Duration::from_secs(20));
        req_res_config.set_connection_keep_alive(Duration::from_secs(20));

        ForestBehaviour {
            gossipsub: Gossipsub::new(
                MessageAuthenticity::Signed(local_key.clone()),
                gossipsub_config,
            ),
            mdns: mdns_opt.into(),
            ping: Ping::default(),
            identify: Identify::new(
                "ipfs/0.1.0".into(),
                // TODO update to include actual version
                // https://github.com/ChainSafe/forest/issues/934
                format!("forest-{}", "0.1.0"),
                local_key.public(),
            ),
            kademlia: kademlia_opt.into(),
            bitswap,
            hello: RequestResponse::new(HelloCodec::default(), hp, req_res_config.clone()),
            chain_exchange: RequestResponse::new(ChainExchangeCodec::default(), cp, req_res_config),
            cx_pending_responses: Default::default(),
            cx_request_table: Default::default(),
            events: vec![],
            peers: Default::default(),
        }
    }

    /// Bootstrap Kademlia network
    pub fn bootstrap(&mut self) -> Result<QueryId, String> {
        if let Some(active_kad) = self.kademlia.as_mut() {
            active_kad.bootstrap().map_err(|e| e.to_string())
        } else {
            Err("Kademlia is not activated".to_string())
        }
    }

    /// Publish data over the gossip network.
    pub fn publish(&mut self, topic: &Topic, data: impl Into<Vec<u8>>) -> Result<(), PublishError> {
        self.gossipsub.publish(topic, data)
    }

    /// Subscribe to a gossip topic.
    pub fn subscribe(&mut self, topic: Topic) -> bool {
        self.gossipsub.subscribe(topic)
    }

    /// Send a hello request or response to some peer.
    pub fn send_hello_request(&mut self, peer_id: &PeerId, request: HelloRequest) {
        self.hello.send_request(peer_id, request);
    }

    /// Send a chain exchange request or response to some peer.
    pub fn send_chain_exchange_request(
        &mut self,
        peer_id: &PeerId,
        request: ChainExchangeRequest,
        response_channel: OneShotSender<Result<ChainExchangeResponse, RequestResponseError>>,
    ) {
        let req_id = self.chain_exchange.send_request(peer_id, request);
        self.cx_request_table.insert(req_id, response_channel);
    }

    /// Adds peer to the peer set.
    pub fn add_peer(&mut self, peer_id: PeerId) {
        if self.peers.insert(peer_id.clone()) {
            self.bitswap.connect(peer_id.clone());
            self.events.push(ForestBehaviourEvent::PeerDialed(peer_id));
        }
    }

    /// Adds peer to the peer set.
    pub fn remove_peer(&mut self, peer_id: &PeerId) {
        self.peers.remove(peer_id);
    }

    /// Adds peer to the peer set.
    pub fn peers(&self) -> &HashSet<PeerId> {
        &self.peers
    }

    /// Send a block to a peer over bitswap
    pub fn send_block(
        &mut self,
        peer_id: &PeerId,
        cid: Cid,
        data: Box<[u8]>,
    ) -> Result<(), Box<dyn Error>> {
        debug!("send {}", cid.to_string());
        let cid = cid.to_bytes();
        let cid = Cid2::try_from(cid)?;
        self.bitswap.send_block(peer_id, cid, data);
        Ok(())
    }

    /// Send a request for data over bitswap
    pub fn want_block(&mut self, cid: Cid, priority: Priority) -> Result<(), Box<dyn Error>> {
        debug!("want {}", cid.to_string());
        let cid = cid.to_bytes();
        let cid = Cid2::try_from(cid)?;
        self.bitswap.want_block(cid, priority);
        Ok(())
    }

    /// Cancel a bitswap request
    pub fn cancel_block(&mut self, cid: &Cid) -> Result<(), Box<dyn Error>> {
        debug!("cancel {}", cid.to_string());
        let cid = cid.to_bytes();
        let cid = Cid2::try_from(cid)?;
        self.bitswap.cancel_block(&cid);
        Ok(())
    }
}
