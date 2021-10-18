// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::chain_exchange::{
    make_chain_exchange_response, ChainExchangeRequest, ChainExchangeResponse,
};
use super::{ForestBehaviour, ForestBehaviourEvent, Libp2pConfig};
use crate::{
    hello::{HelloRequest, HelloResponse},
    rpc::RequestResponseError,
};
use async_std::channel::{unbounded, Receiver, Sender};
use async_std::{stream, task};
use chain::ChainStore;
use forest_blocks::GossipBlock;
use forest_cid::{Cid, Code::Blake2b256};
use forest_encoding::from_slice;
use forest_message::SignedMessage;
use futures::channel::oneshot::Sender as OneShotSender;
use futures::select;
use futures_util::stream::StreamExt;
use ipld_blockstore::BlockStore;
pub use libp2p::gossipsub::IdentTopic;
pub use libp2p::gossipsub::Topic;
use libp2p::multiaddr::Protocol;
use libp2p::multihash::Multihash;
use libp2p::request_response::ResponseChannel;
use libp2p::swarm::SwarmEvent;
use libp2p::{
    core,
    core::connection::ConnectionLimits,
    core::muxing::StreamMuxerBox,
    core::transport::Boxed,
    identity::{ed25519, Keypair},
    mplex, noise, yamux, PeerId, Swarm, Transport,
};
use libp2p::{core::Multiaddr, swarm::SwarmBuilder};
use log::{debug, error, info, trace, warn};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use utils::read_file_to_vec;

/// Gossipsub Filecoin blocks topic identifier.
pub const PUBSUB_BLOCK_STR: &str = "/fil/blocks";
/// Gossipsub Filecoin messages topic identifier.
pub const PUBSUB_MSG_STR: &str = "/fil/msgs";

const PUBSUB_TOPICS: [&str; 2] = [PUBSUB_BLOCK_STR, PUBSUB_MSG_STR];

/// Events emitted by this Service.
#[derive(Debug)]
pub enum NetworkEvent {
    PubsubMessage {
        source: PeerId,
        message: PubsubMessage,
    },
    HelloRequest {
        request: HelloRequest,
        source: PeerId,
    },
    ChainExchangeRequest {
        request: ChainExchangeRequest,
        channel: ResponseChannel<ChainExchangeResponse>,
    },
    PeerConnected(PeerId),
    PeerDisconnected(PeerId),
    BitswapBlock {
        cid: Cid,
    },
}

/// Message types that can come over GossipSub
#[derive(Debug, Clone)]
pub enum PubsubMessage {
    /// Messages that come over the block topic
    Block(GossipBlock),
    /// Messages that come over the message topic
    Message(SignedMessage),
}

/// Messages into the service to handle.
#[derive(Debug)]
pub enum NetworkMessage {
    PubsubMessage {
        topic: IdentTopic,
        message: Vec<u8>,
    },
    ChainExchangeRequest {
        peer_id: PeerId,
        request: ChainExchangeRequest,
        response_channel: OneShotSender<Result<ChainExchangeResponse, RequestResponseError>>,
    },
    HelloRequest {
        peer_id: PeerId,
        request: HelloRequest,
        response_channel: OneShotSender<Result<HelloResponse, RequestResponseError>>,
    },
    BitswapRequest {
        cid: Cid,
        response_channel: OneShotSender<()>,
    },
    JSONRPCRequest {
        method: NetRPCMethods,
    },
}

/// Network RPC API methods used to gather data from libp2p node.
#[derive(Debug)]
pub enum NetRPCMethods {
    NetAddrsListen(OneShotSender<(PeerId, Vec<Multiaddr>)>),
    NetPeers(OneShotSender<HashMap<PeerId, Vec<Multiaddr>>>),
    NetConnect(OneShotSender<bool>, PeerId, Vec<Multiaddr>),
    NetDisconnect(OneShotSender<()>, PeerId),
}

/// The Libp2pService listens to events from the Libp2p swarm.
pub struct Libp2pService<DB> {
    swarm: Swarm<ForestBehaviour>,
    cs: Arc<ChainStore<DB>>,

    network_receiver_in: Receiver<NetworkMessage>,
    network_sender_in: Sender<NetworkMessage>,
    network_receiver_out: Receiver<NetworkEvent>,
    network_sender_out: Sender<NetworkEvent>,
    network_name: String,
    bitswap_response_channels: HashMap<Cid, Vec<OneShotSender<()>>>,
}

impl<DB> Libp2pService<DB>
where
    DB: BlockStore + Sync + Send + 'static,
{
    pub fn new(
        config: Libp2pConfig,
        cs: Arc<ChainStore<DB>>,
        net_keypair: Keypair,
        network_name: &str,
    ) -> Self {
        let peer_id = PeerId::from(net_keypair.public());

        let transport = build_transport(net_keypair.clone());

        let limits = ConnectionLimits::default()
            .with_max_pending_incoming(Some(10))
            .with_max_pending_outgoing(Some(30))
            .with_max_established_incoming(Some(config.target_peer_count))
            .with_max_established_outgoing(Some(config.target_peer_count))
            .with_max_established_per_peer(Some(5));

        let mut swarm = SwarmBuilder::new(
            transport,
            ForestBehaviour::new(&net_keypair, &config, network_name),
            peer_id,
        )
        .connection_limits(limits)
        .notify_handler_buffer_size(std::num::NonZeroUsize::new(20).expect("Not zero"))
        .connection_event_buffer_size(64)
        .build();

        Swarm::listen_on(&mut swarm, config.listening_multiaddr).unwrap();

        // Subscribe to gossipsub topics with the network name suffix
        for topic in PUBSUB_TOPICS.iter() {
            let t = Topic::new(format!("{}/{}", topic, network_name));
            swarm.behaviour_mut().subscribe(&t).unwrap();
        }

        // Bootstrap with Kademlia
        if let Err(e) = swarm.behaviour_mut().bootstrap() {
            warn!("Failed to bootstrap with Kademlia: {}", e);
        }

        let (network_sender_in, network_receiver_in) = unbounded();
        let (network_sender_out, network_receiver_out) = unbounded();

        Libp2pService {
            swarm,
            cs,
            network_receiver_in,
            network_sender_in,
            network_receiver_out,
            network_sender_out,
            network_name: network_name.to_owned(),
            bitswap_response_channels: Default::default(),
        }
    }

    /// Starts the libp2p service networking stack. This Future resolves when shutdown occurs.
    pub async fn run(mut self) {
        let mut swarm_stream = self.swarm.fuse();
        let mut network_stream = self.network_receiver_in.fuse();
        let mut interval = stream::interval(Duration::from_secs(15)).fuse();
        let pubsub_block_str = format!("{}/{}", PUBSUB_BLOCK_STR, self.network_name);
        let pubsub_msg_str = format!("{}/{}", PUBSUB_MSG_STR, self.network_name);

        loop {
            select! {
                swarm_event = swarm_stream.next() => match swarm_event {
                    // outbound events
                    Some(event) => match event {
                        SwarmEvent::Behaviour(ForestBehaviourEvent::PeerConnected(peer_id)) => {
                            debug!("Peer connected, {:?}", peer_id);
                            emit_event(&self.network_sender_out,
                                NetworkEvent::PeerConnected(peer_id)).await;
                        }
                        SwarmEvent::Behaviour(ForestBehaviourEvent::PeerDisconnected(peer_id)) => {
                            emit_event(&self.network_sender_out, NetworkEvent::PeerDisconnected(peer_id)).await;
                        }
                        SwarmEvent::Behaviour(ForestBehaviourEvent::GossipMessage {
                            source,
                            topic,
                            message,
                        }) => {
                            trace!("Got a Gossip Message from {:?}", source);
                            let topic = topic.as_str();
                            if topic == pubsub_block_str {
                                match from_slice::<GossipBlock>(&message) {
                                    Ok(b) => {
                                        emit_event(&self.network_sender_out, NetworkEvent::PubsubMessage{
                                            source,
                                            message: PubsubMessage::Block(b),
                                        }).await;
                                    }
                                    Err(e) => {
                                        warn!("Gossip Block from peer {:?} could not be deserialized: {}", source, e);
                                    }
                                }
                            } else if topic == pubsub_msg_str {
                                match from_slice::<SignedMessage>(&message) {
                                    Ok(m) => {
                                        emit_event(&self.network_sender_out, NetworkEvent::PubsubMessage{
                                            source,
                                            message: PubsubMessage::Message(m),
                                        }).await;
                                    }
                                    Err(e) => {
                                        warn!("Gossip Message from peer {:?} could not be deserialized: {}", source, e);
                                    }
                                }
                            } else {
                                warn!("Getting gossip messages from unknown topic: {}", topic);
                            }
                        }
                        SwarmEvent::Behaviour(ForestBehaviourEvent::HelloRequest { request,  peer } )=> {
                            debug!("Received hello request (peer_id: {:?})", peer);
                            emit_event(&self.network_sender_out, NetworkEvent::HelloRequest {
                                request,
                                source: peer,
                            }).await;
                        }
                        SwarmEvent::Behaviour(ForestBehaviourEvent::ChainExchangeRequest { channel, peer, request }) => {
                            debug!("Received chain_exchange request (peer_id: {:?})", peer);
                            let db = self.cs.clone();

                            task::spawn(async move {
                                channel.send(make_chain_exchange_response(db.as_ref(), &request).await)
                            });
                        }
                        SwarmEvent::Behaviour(ForestBehaviourEvent::BitswapReceivedBlock(_peer_id, cid, block)) => {
                            let res: Result<_, String> = self.cs.blockstore().put_raw(block.into(), Blake2b256).map_err(|e| e.to_string());
                            match res {
                                Ok(actual_cid) => {
                                    if actual_cid != cid {
                                        warn!("Bitswap cid mismatch: cid {:?}, expected cid: {:?}", actual_cid, cid);
                                    } else if let Some (chans) = self.bitswap_response_channels.remove(&cid) {
                                            for chan in chans.into_iter(){
                                                if chan.send(()).is_err() {
                                                    debug!("Bitswap response channel send failed");
                                                }
                                                trace!("Saved Bitswap block with cid {:?}", cid);
                                        }
                                    } else {
                                        debug!("Received Bitswap response, but response channel cannot be found");
                                    }
                                    emit_event(&self.network_sender_out, NetworkEvent::BitswapBlock{cid}).await;
                                }
                                Err(e) => {
                                    warn!("failed to save bitswap block: {:?}", e.to_string());
                                }
                            }
                        },
                        SwarmEvent::Behaviour(ForestBehaviourEvent::BitswapReceivedWant(peer_id, cid,)) => match self.cs.blockstore().get(&cid) {
                            Ok(Some(data)) => {
                                match swarm_stream.get_mut().behaviour_mut().send_block(&peer_id, cid, data) {
                                    Ok(_) => {
                                        trace!("Sent bitswap message successfully");
                                    },
                                    Err(e) => {
                                        warn!("Failed to send Bitswap reply: {}", e.to_string());
                                    },
                                }
                            }
                            Ok(None) => {
                                trace!("Don't have data for: {}", cid);
                            }
                            Err(e) => {
                                trace!("Failed to get data: {}", e.to_string());
                            }
                        },
                        _ => {
                            continue;
                        }
                    }
                    None => { break; }
                },
                rpc_message = network_stream.next() => match rpc_message {
                    // Inbound messages
                    Some(message) =>  match message {
                        NetworkMessage::PubsubMessage { topic, message } => {
                            if let Err(e) = swarm_stream.get_mut().behaviour_mut().publish(topic, message) {
                                warn!("Failed to send gossipsub message: {:?}", e);
                            }
                        }
                        NetworkMessage::HelloRequest { peer_id, request, response_channel } => {
                            swarm_stream.get_mut().behaviour_mut().send_hello_request(&peer_id, request, response_channel);
                        }
                        NetworkMessage::ChainExchangeRequest { peer_id, request, response_channel } => {
                            swarm_stream.get_mut().behaviour_mut().send_chain_exchange_request(&peer_id, request, response_channel);
                        }
                        NetworkMessage::BitswapRequest { cid, response_channel } => {
                            if let Err(e) = swarm_stream.get_mut().behaviour_mut().want_block(cid, 1000) {
                                warn!("Failed to send a bitswap want_block: {}", e.to_string());
                            } else if let Some(chans) = self.bitswap_response_channels.get_mut(&cid) {
                                    chans.push(response_channel);
                            } else {
                                self.bitswap_response_channels.insert(cid, vec![response_channel]);
                            }
                        }
                        NetworkMessage::JSONRPCRequest { method } => {
                            match method {
                                NetRPCMethods::NetAddrsListen(response_channel) => {
                                    let listeners: Vec<_> = Swarm::listeners( swarm_stream.get_mut()).cloned().collect();
                                    let peer_id = Swarm::local_peer_id(swarm_stream.get_mut());

                                    if response_channel.send((*peer_id, listeners)).is_err() {
                                        warn!("Failed to get Libp2p listeners");
                                    }
                                }
                                NetRPCMethods::NetPeers(response_channel) => {
                                    let peer_addresses: &HashMap<PeerId, Vec<Multiaddr>> = swarm_stream.get_mut().behaviour_mut().peer_addresses();

                                    if response_channel.send(peer_addresses.to_owned()).is_err() {
                                        warn!("Failed to get Libp2p peers");
                                    }
                                }
                                NetRPCMethods::NetConnect(response_channel, peer_id, addresses) => {
                                    let mut addresses = addresses.clone();
                                    let mut success = false;

                                    for multiaddr in addresses.iter_mut() {
                                        multiaddr.push(Protocol::P2p(Multihash::from_bytes(&peer_id.to_bytes()).unwrap()));

                                        if Swarm::dial_addr(swarm_stream.get_mut(), multiaddr.clone()).is_ok() {
                                            success = true;
                                            break;
                                        };
                                    }

                                    if response_channel.send(success).is_err() {
                                        warn!("Failed to connect to a peer");
                                    }
                                }
                                NetRPCMethods::NetDisconnect(response_channel, _peer_id) => {
                                    warn!("NetDisconnect API not yet implmeneted"); // TODO: implement NetDisconnect - See #1181

                                    if response_channel.send(()).is_err() {
                                        warn!("Failed to disconnect from a peer");
                                    }
                                }
                            }
                        }
                    }
                    None => { break; }
                },
                interval_event = interval.next() => if interval_event.is_some() {
                    // Print peer count on an interval.
                    info!("Peers connected: {}", swarm_stream.get_mut().behaviour_mut().peers().len());
                }
            };
        }
    }

    /// Returns a sender which allows sending messages to the libp2p service.
    pub fn network_sender(&self) -> Sender<NetworkMessage> {
        self.network_sender_in.clone()
    }

    /// Returns a receiver to listen to network events emitted from the service.
    pub fn network_receiver(&self) -> Receiver<NetworkEvent> {
        self.network_receiver_out.clone()
    }
}

async fn emit_event(sender: &Sender<NetworkEvent>, event: NetworkEvent) {
    if sender.send(event).await.is_err() {
        error!("Failed to emit event: Network channel receiver has been dropped");
    }
}

/// Builds the transport stack that LibP2P will communicate over.
pub fn build_transport(local_key: Keypair) -> Boxed<(PeerId, StreamMuxerBox)> {
    let transport = libp2p::tcp::TcpConfig::new().nodelay(true);
    let transport = libp2p::websocket::WsConfig::new(transport.clone()).or_transport(transport);
    let transport = async_std::task::block_on(libp2p::dns::DnsConfig::system(transport)).unwrap();
    let auth_config = {
        let dh_keys = noise::Keypair::<noise::X25519Spec>::new()
            .into_authentic(&local_key)
            .expect("Noise key generation failed");

        noise::NoiseConfig::xx(dh_keys).into_authenticated()
    };

    let mplex_config = {
        let mut mplex_config = mplex::MplexConfig::new();
        mplex_config.set_max_buffer_size(usize::MAX);

        let mut yamux_config = yamux::YamuxConfig::default();
        yamux_config.set_max_buffer_size(16 * 1024 * 1024);
        yamux_config.set_receive_window_size(16 * 1024 * 1024);
        // yamux_config.set_window_update_mode(WindowUpdateMode::OnRead);
        core::upgrade::SelectUpgrade::new(yamux_config, mplex_config)
    };

    transport
        .upgrade(core::upgrade::Version::V1)
        .authenticate(auth_config)
        .multiplex(mplex_config)
        .timeout(Duration::from_secs(20))
        .boxed()
}

/// Fetch keypair from disk, returning none if it cannot be decoded.
pub fn get_keypair(path: &Path) -> Option<Keypair> {
    match read_file_to_vec(path) {
        Err(e) => {
            info!("Networking keystore not found!");
            trace!("Error {:?}", e);
            None
        }
        Ok(mut vec) => match ed25519::Keypair::decode(&mut vec) {
            Ok(kp) => {
                info!("Recovered libp2p keypair from {:?}", &path);
                Some(Keypair::Ed25519(kp))
            }
            Err(e) => {
                info!("Could not decode networking keystore!");
                trace!("Error {:?}", e);
                None
            }
        },
    }
}
