// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::behaviour::{MyBehaviour, MyBehaviourEvent};
use super::config::Libp2pConfig;
use futures::{Async, Stream};
use libp2p::{
    core,
    core::muxing::StreamMuxerBox,
    core::nodes::Substream,
    core::transport::boxed::Boxed,
    gossipsub::TopicHash,
    identity::{ed25519, Keypair},
    mplex, secio, yamux, PeerId, Swarm, Transport,
};
use slog::{debug, error, info, trace, Logger};
use std::io::{Error, ErrorKind};
use std::time::Duration;
use utils::{get_home_dir, read_file_to_vec, write_to_file};
type Libp2pStream = Boxed<(PeerId, StreamMuxerBox), Error>;
type Libp2pBehaviour = MyBehaviour<Substream<StreamMuxerBox>>;

/// The Libp2pService listens to events from the Libp2p swarm.
pub struct Libp2pService {
    pub swarm: Swarm<Libp2pStream, Libp2pBehaviour>,
}

impl Libp2pService {
    /// Constructs a Libp2pService
    pub fn new(log: &Logger, config: &Libp2pConfig) -> Self {
        let net_keypair = get_keypair(log);
        let peer_id = PeerId::from(net_keypair.public());

        info!(log, "Local peer id: {:?}", peer_id);

        let transport = build_transport(net_keypair.clone());

        let mut swarm = {
            let be = MyBehaviour::new(log.clone(), &net_keypair);
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

        Libp2pService { swarm }
    }
}

impl Stream for Libp2pService {
    type Item = NetworkEvent;
    type Error = ();

    /// Continuously polls the Libp2p swarm to get events
    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        loop {
            match self.swarm.poll() {
                Ok(Async::Ready(Some(event))) => match event {
                    MyBehaviourEvent::DiscoveredPeer(peer) => {
                        libp2p::Swarm::dial(&mut self.swarm, peer);
                    }
                    MyBehaviourEvent::ExpiredPeer(_) => {}
                    MyBehaviourEvent::GossipMessage {
                        source,
                        topics,
                        message,
                    } => {
                        return Ok(Async::Ready(Option::from(NetworkEvent::PubsubMessage {
                            source,
                            topics,
                            message,
                        })));
                    }
                },
                Ok(Async::Ready(None)) => break,
                Ok(Async::NotReady) => break,
                _ => break,
            }
        }
        Ok(Async::NotReady)
    }
}

/// Events emitted by this Service to be listened by the NetworkService.
#[derive(Clone)]
pub enum NetworkEvent {
    PubsubMessage {
        source: PeerId,
        topics: Vec<TopicHash>,
        message: Vec<u8>,
    },
}

pub fn build_transport(local_key: Keypair) -> Boxed<(PeerId, StreamMuxerBox), Error> {
    let transport = libp2p::tcp::TcpConfig::new().nodelay(true);
    let transport = libp2p::dns::DnsConfig::new(transport);

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

/// Fetch keypair from disk, or generate a new one if its not available
fn get_keypair(log: &Logger) -> Keypair {
    let path_to_keystore = get_home_dir() + "/.forest/libp2p/keypair";
    let local_keypair = match read_file_to_vec(&path_to_keystore) {
        Err(e) => {
            info!(log, "Networking keystore not found!");
            trace!(log, "Error {:?}", e);
            return generate_new_peer_id(log);
        }
        Ok(mut vec) => {
            // If decoding fails, generate new peer id
            // TODO rename old file to keypair.old(?)
            match ed25519::Keypair::decode(&mut vec) {
                Ok(kp) => {
                    info!(log, "Recovered keystore from {:?}", &path_to_keystore);
                    kp
                }
                Err(e) => {
                    info!(log, "Could not decode networking keystore!");
                    trace!(log, "Error {:?}", e);
                    return generate_new_peer_id(log);
                }
            }
        }
    };

    Keypair::Ed25519(local_keypair)
}

/// Generates a new libp2p keypair and saves to disk
fn generate_new_peer_id(log: &Logger) -> Keypair {
    let path_to_keystore = get_home_dir() + "/.forest/libp2p/";
    let generated_keypair = Keypair::generate_ed25519();
    info!(log, "Generated new keystore!");

    if let Keypair::Ed25519(key) = generated_keypair.clone() {
        if let Err(e) = write_to_file(&key.encode(), &path_to_keystore, "keypair") {
            info!(log, "Could not write keystore to disk!");
            trace!(log, "Error {:?}", e);
        };
    }

    generated_keypair
}
