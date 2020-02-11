// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::behaviour::{ForestBehaviour, ForestBehaviourEvent};
use super::config::Libp2pConfig;
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
use slog::{debug, error, info, trace, Logger};
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
}

/// Events into this Service
#[derive(Clone, Debug)]
pub enum NetworkMessage {
    PubsubMessage { topic: Topic, message: Vec<u8> },
}
/// The Libp2pService listens to events from the Libp2p swarm.
pub struct Libp2pService {
    swarm: Swarm<Libp2pStream, Libp2pBehaviour>,

    pubsub_receiver_in: Receiver<NetworkMessage>,
    pubsub_sender_in: Sender<NetworkMessage>,

    pubsub_receiver_out: Receiver<NetworkEvent>,
    pubsub_sender_out: Sender<NetworkEvent>,

    log: Logger,
}

impl Libp2pService {
    /// Constructs a Libp2pService
    pub fn new(log: Logger, config: &Libp2pConfig, net_keypair: Keypair) -> Self {
        let peer_id = PeerId::from(net_keypair.public());

        info!(log, "Local peer id: {:?}", peer_id);

        let transport = build_transport(net_keypair.clone());

        let mut swarm = {
            let be = ForestBehaviour::new(log.clone(), &net_keypair);
            Swarm::new(transport, be, peer_id)
        };

        for node in config.bootstrap_peers.clone() {
            match node.parse() {
                Ok(to_dial) => match Swarm::dial_addr(&mut swarm, to_dial) {
                    Ok(_) => debug!(log, "Dialed {:?}", node),
                    Err(e) => debug!(log, "Dial {:?} failed: {:?}", node, e),
                },
                Err(err) => error!(log, "Failed to parse address to dial: {:?}", err),
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

        let (pubsub_sender_in, pubsub_receiver_in) = channel(20);
        let (pubsub_sender_out, pubsub_receiver_out) = channel(20);
        Libp2pService {
            swarm,
            pubsub_receiver_in,
            pubsub_sender_in,
            pubsub_receiver_out,
            pubsub_sender_out,
            log,
        }
    }

    /// Starts the `Libp2pService` networking stack. This Future resolves when shutdown occurs.
    pub async fn run(self) {
        let mut swarm_stream = self.swarm.fuse();
        let mut pubsub_stream = self.pubsub_receiver_in.fuse();
        loop {
            select! {
                swarm_event = swarm_stream.next() => match swarm_event {
                    Some(event) => match event {
                        ForestBehaviourEvent::DiscoveredPeer(peer) => {
                            libp2p::Swarm::dial(&mut swarm_stream.get_mut(), peer);
                        }
                        ForestBehaviourEvent::ExpiredPeer(_) => {}
                        ForestBehaviourEvent::GossipMessage {
                            source,
                            topics,
                            message,
                        } => {
                            info!(self.log, "Got a Gossip Message from {:?}", source);
                            self.pubsub_sender_out.send(NetworkEvent::PubsubMessage {
                                source,
                                topics,
                                message
                            }).await;
                        }
                    }
                    None => {break;}
                },
                rpc_message = pubsub_stream.next() => match rpc_message {
                    Some(message) =>  match message {
                        NetworkMessage::PubsubMessage{topic, message} => {
                            swarm_stream.get_mut().publish(&topic, message);
                        }
                    }
                    None => {break;}
                }
            };
        }
    }

    /// Returns a `Sender` allowing you to send messages over GossipSub
    pub fn pubsub_sender(&self) -> Sender<NetworkMessage> {
        self.pubsub_sender_in.clone()
    }

    /// Returns a `Receiver` to listen to GossipSub messages
    pub fn pubsub_receiver(&self) -> Receiver<NetworkEvent> {
        self.pubsub_receiver_out.clone()
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
pub fn get_keypair(log: &Logger, path: &str) -> Option<Keypair> {
    let path_to_keystore = get_home_dir() + path;
    match read_file_to_vec(&path_to_keystore) {
        Err(e) => {
            info!(log, "Networking keystore not found!");
            trace!(log, "Error {:?}", e);
            None
        }
        Ok(mut vec) => match ed25519::Keypair::decode(&mut vec) {
            Ok(kp) => {
                info!(log, "Recovered keystore from {:?}", &path_to_keystore);
                Some(Keypair::Ed25519(kp))
            }
            Err(e) => {
                info!(log, "Could not decode networking keystore!");
                trace!(log, "Error {:?}", e);
                None
            }
        },
    }
}
