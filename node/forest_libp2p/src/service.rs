// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::blocksync::BlockSyncResponse;
use super::hello::HelloMessage;
use super::rpc::{RPCEvent, RPCRequest, RPCResponse};
use super::{ForestBehaviour, ForestBehaviourEvent, Libp2pConfig};
use async_std::sync::{channel, Receiver, Sender};
use futures::select;
use futures_util::stream::StreamExt;
use libp2p::{
    core,
    core::muxing::StreamMuxerBox,
    core::nodes::Substream,
    core::transport::boxed::Boxed,
    gossipsub::{Topic, TopicHash},
    identity::{ed25519, Keypair},
    mplex, secio, yamux, PeerId, Swarm, Transport,
};
use log::{debug, error, info, trace};
use std::io::{Error, ErrorKind};
use std::time::Duration;
use utils::{get_home_dir, read_file_to_vec};

type Libp2pStream = Boxed<(PeerId, StreamMuxerBox), Error>;
type Libp2pBehaviour = ForestBehaviour<Substream<StreamMuxerBox>>;

/// Events emitted by this Service
#[derive(Clone, Debug)]
pub enum NetworkEvent {
    PubsubMessage {
        source: PeerId,
        topics: Vec<TopicHash>,
        message: Vec<u8>,
    },
    RPCRequest {
        req_id: usize,
        request: RPCRequest,
    },
    RPCResponse {
        req_id: usize,
        response: RPCResponse,
    },
    Hello {
        source: PeerId,
        message: HelloMessage,
    },
}

/// Events into this Service
#[derive(Clone, Debug)]
pub enum NetworkMessage {
    PubsubMessage { topic: Topic, message: Vec<u8> },
    RPC { peer_id: PeerId, event: RPCEvent },
}
/// The Libp2pService listens to events from the Libp2p swarm.
pub struct Libp2pService {
    pub swarm: Swarm<Libp2pStream, Libp2pBehaviour>,

    network_receiver_in: Receiver<NetworkMessage>,
    network_sender_in: Sender<NetworkMessage>,
    network_receiver_out: Receiver<NetworkEvent>,
    network_sender_out: Sender<NetworkEvent>,
}

impl Libp2pService {
    /// Constructs a Libp2pService
    pub fn new(config: &Libp2pConfig, net_keypair: Keypair) -> Self {
        let peer_id = PeerId::from(net_keypair.public());

        info!("Local peer id: {:?}", peer_id);

        let transport = build_transport(net_keypair.clone());

        let mut swarm = {
            let be = ForestBehaviour::new(&net_keypair);
            Swarm::new(transport, be, peer_id)
        };

        for node in config.bootstrap_peers.clone() {
            match node.parse() {
                Ok(to_dial) => match Swarm::dial_addr(&mut swarm, to_dial) {
                    Ok(_) => debug!("Dialed {:?}", node),
                    Err(e) => debug!("Dial {:?} failed: {:?}", node, e),
                },
                Err(err) => error!("Failed to parse address to dial: {:?}", err),
            }
        }

        Swarm::listen_on(
            &mut swarm,
            config
                .listening_multiaddr
                .parse()
                .expect("Incorrect MultiAddr Format"),
        )
        .unwrap();

        for topic in config.pubsub_topics.clone() {
            swarm.subscribe(topic);
        }

        let (network_sender_in, network_receiver_in) = channel(20);
        let (network_sender_out, network_receiver_out) = channel(20);
        Libp2pService {
            swarm,
            network_receiver_in,
            network_sender_in,
            network_receiver_out,
            network_sender_out,
        }
    }

    /// Starts the `Libp2pService` networking stack. This Future resolves when shutdown occurs.
    pub async fn run(self) {
        let mut swarm_stream = self.swarm.fuse();
        let mut network_stream = self.network_receiver_in.fuse();
        loop {
            select! {
                swarm_event = swarm_stream.next() => match swarm_event {
                    Some(event) => match event {
                        ForestBehaviourEvent::PeerDialed(peer_id) => {
                            info!("Peer dialed, {:?}", peer_id);
                            // TODO add sending hello after genesis setup
                        }
                        ForestBehaviourEvent::PeerDisconnected(peer_id) => {
                            info!("Peer disconnected, {:?}", peer_id);
                        }
                        ForestBehaviourEvent::DiscoveredPeer(peer) => {
                            info!("Discovered: {:?}", peer);
                            libp2p::Swarm::dial(&mut swarm_stream.get_mut(), peer);
                        }
                        ForestBehaviourEvent::ExpiredPeer(_) => {}
                        ForestBehaviourEvent::GossipMessage {
                            source,
                            topics,
                            message,
                        } => {
                            info!("Got a Gossip Message from {:?}", source);
                            self.network_sender_out.send(NetworkEvent::PubsubMessage {
                                source,
                                topics,
                                message
                            }).await;
                        }
                        ForestBehaviourEvent::RPC(peer_id, event) => {
                            info!("RPC event {:?}", event);
                            match event {
                                RPCEvent::Response(req_id, res) => {
                                    self.network_sender_out.send(NetworkEvent::RPCResponse {
                                        req_id,
                                        response: res,
                                    }).await;
                                }
                                RPCEvent::Request(req_id, RPCRequest::BlockSync(r)) => {
                                    // TODO implement handling incoming blocksync requests
                                    swarm_stream.get_mut().send_rpc(peer_id, RPCEvent::Response(1, RPCResponse::BlockSync(BlockSyncResponse {
                                        chain: vec![],
                                        status: 203,
                                        message: "handling requests not implemented".to_owned(),
                                    })));
                                }
                                RPCEvent::Request(req_id, RPCRequest::Hello(message)) => {
                                    self.network_sender_out.send(NetworkEvent::Hello{
                                        message, source: peer_id}).await;
                                }
                                RPCEvent::Error(req_id, err) => info!("Error with request {}: {:?}", req_id, err),
                            }
                        }
                    }
                    None => {break;}
                },
                rpc_message = network_stream.next() => match rpc_message {
                    Some(message) =>  match message {
                        NetworkMessage::PubsubMessage{topic, message} => {
                            swarm_stream.get_mut().publish(&topic, message);
                        }
                        NetworkMessage::RPC{peer_id, event} => {
                            swarm_stream.get_mut().send_rpc(peer_id, event);
                        }
                    }
                    None => {break;}
                }
            };
        }
    }

    /// Returns a `Sender` allowing you to send messages over GossipSub
    pub fn network_sender(&self) -> Sender<NetworkMessage> {
        self.network_sender_in.clone()
    }

    /// Returns a `Receiver` to listen to network events
    pub fn network_receiver(&self) -> Receiver<NetworkEvent> {
        self.network_receiver_out.clone()
    }
}

/// Builds the transport stack that LibP2P will communicate over
pub fn build_transport(local_key: Keypair) -> Boxed<(PeerId, StreamMuxerBox), Error> {
    let transport = libp2p::tcp::TcpConfig::new().nodelay(true);
    let transport = libp2p::dns::DnsConfig::new(transport).unwrap();
    transport
        .upgrade(core::upgrade::Version::V1)
        .authenticate(secio::SecioConfig::new(local_key))
        .multiplex(core::upgrade::SelectUpgrade::new(
            yamux::Config::default(),
            mplex::MplexConfig::new(),
        ))
        .map(|(peer, muxer), _| (peer, core::muxing::StreamMuxerBox::new(muxer)))
        .timeout(Duration::from_secs(20))
        .map_err(|err| Error::new(ErrorKind::Other, err))
        .boxed()
}

/// Fetch keypair from disk, returning none if it cannot be decoded
pub fn get_keypair(path: &str) -> Option<Keypair> {
    let path_to_keystore = get_home_dir() + path;
    match read_file_to_vec(&path_to_keystore) {
        Err(e) => {
            info!("Networking keystore not found!");
            trace!("Error {:?}", e);
            None
        }
        Ok(mut vec) => match ed25519::Keypair::decode(&mut vec) {
            Ok(kp) => {
                info!("Recovered keystore from {:?}", &path_to_keystore);
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
