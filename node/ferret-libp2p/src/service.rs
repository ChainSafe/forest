// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use super::behaviour::{MyBehaviour, MyBehaviourEvent};
use super::config::Libp2pConfig;
use async_std::task;
use futures::channel::mpsc;
use futures::stream::Stream;
use futures::task::{Context, Poll};
use futures_util::stream::StreamExt;
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
use std::sync::{Mutex, Arc};
use std::pin::Pin;
use std::time::Duration;
use utils::{get_home_dir, read_file_to_vec, write_to_file};

type Libp2pStream = Boxed<(PeerId, StreamMuxerBox), Error>;
type Libp2pBehaviour = MyBehaviour<Substream<StreamMuxerBox>>;

/// Events emitted by this Service
#[derive(Clone, Debug)]
pub enum NetworkEvent {
    PubsubMessage {
        source: PeerId,
        topics: Vec<TopicHash>,
        message: Vec<u8>,
    },
}
/// The Libp2pService listens to events from the Libp2p swarm.
pub struct Libp2pService {
    swarm: Swarm<Libp2pStream, Libp2pBehaviour>,
    libp2p_receiver: mpsc::UnboundedReceiver<u8>,
    libp2p_sender: mpsc::UnboundedSender<u8>,
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

        let (libp2p_sender, libp2p_receiver) = mpsc::unbounded();
        Libp2pService { swarm:swarm, libp2p_sender, libp2p_receiver }
//        Libp2pService { swarm: Arc::new(Mutex::new(swarm)), libp2p_sender, libp2p_receiver }
    }

    pub async fn run(self) -> Result<(), ()> {
        enum MergeEvent {
            Swarm(MyBehaviourEvent),
            Pubsub,
        };
//        let mut swarm = self.swarm.clone();
        let mut source = self.swarm.map(MergeEvent::Swarm);
        loop {
            match source.next().await.unwrap() {
                MergeEvent::Swarm(event) => match event {
                    MyBehaviourEvent::DiscoveredPeer(peer) => {
                        libp2p::Swarm::dial(&mut source.get_mut(), peer);
                    }
                    MyBehaviourEvent::ExpiredPeer(_) => {}
                    MyBehaviourEvent::GossipMessage {
                        source,
                        topics,
                        message,
                    } => {
                       println!("PS MSG {:?} {:?} {:?}", source, topics, message);
                    }
                },
                _ => {}
            }
        }
    }
}

//impl Stream for Libp2pService {
//    type Item = NetworkEvent;
//
//    /// Continuously polls the Libp2p swarm to get events
//    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
//        loop {
//            match self.swarm.poll_next_unpin(cx) {
//                Poll::Ready(Some(event)) => match event {
//                    MyBehaviourEvent::DiscoveredPeer(peer) => {
//                        libp2p::Swarm::dial(&mut self.swarm, peer);
//                    }
//                    MyBehaviourEvent::ExpiredPeer(_) => {}
//                    MyBehaviourEvent::GossipMessage {
//                        source,
//                        topics,
//                        message,
//                    } => {
//                        return Poll::Ready(Option::from(NetworkEvent::PubsubMessage {
//                            source,
//                            topics,
//                            message,
//                        }));
//                    }
//                },
//                Poll::Ready(None) => break,
//                Poll::Pending => break,
//                _ => break,
//            }
//        }
//        Poll::Pending
//    }
//}



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

/// Fetch keypair from disk, or generate a new one if its not available
fn get_keypair(log: &Logger) -> Keypair {
    let path_to_keystore = get_home_dir() + "/.ferret1/libp2p/keypair";
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
    let path_to_keystore = get_home_dir() + "/.ferret1/libp2p/";
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
