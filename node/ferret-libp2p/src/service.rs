use crate::behaviour::{MyBehaviour, MyBehaviourEvent};
use futures::{Async, Stream};
use libp2p::{
    self, core,
    core::muxing::StreamMuxerBox,
    core::nodes::Substream,
    core::transport::boxed::Boxed,
    gossipsub::{Topic, TopicHash},
    identity, mplex, secio, yamux, PeerId, Swarm, Transport,
};
use std::io::{Error, ErrorKind};
use std::time::Duration;
use super::config::Libp2pConfig;

type Libp2pStream = Boxed<(PeerId, StreamMuxerBox), Error>;
type Libp2pBehaviour = MyBehaviour<Substream<StreamMuxerBox>>;

pub struct Libp2pService {
    pub swarm: Swarm<Libp2pStream, Libp2pBehaviour>,
}

impl Libp2pService {
    pub fn new(config: &Libp2pConfig) -> Result<Self, Error> {
        // Starting Libp2p Service

        // TODO @Greg do local storage
        let local_key = identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());

        println!("Local peer id: {:?}", local_peer_id);

        let transport = build_transport(local_key.clone());

        let mut swarm = {
            let be = MyBehaviour::new(&local_key);
            Swarm::new(transport, be, local_peer_id)
        };

        for node in config.bootstrap_peers.clone() {
            let dialing = node.clone();
            match node.parse() {
                Ok(to_dial) => {
                    match libp2p::Swarm::dial_addr(
                        &mut swarm,
                        to_dial,
                    ) {
                        Ok(_) => println!("Dialed {:?}", dialing),
                        Err(e) => println!("Dial {:?} failed: {:?}", dialing, e),
                    }
                }
                Err(err) => println!("Failed to parse address to dial: {:?}", err),
            }
        }

        Swarm::listen_on(&mut swarm, config.listening_multiaddr.parse().unwrap()).unwrap();

        for topic in config.pubsub_topics.clone() {
            swarm.subscribe(topic);
        }

        Ok(Libp2pService { swarm })
    }
}

impl Stream for Libp2pService {
    type Item = NetworkEvent;
    type Error = ();

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        let _listening = false;
        loop {
            println!("loop poll");
            match self.swarm.poll() {
                Ok(Async::Ready(Some(event))) => match event {
                    MyBehaviourEvent::DiscoveredPeer(peer) => {
                        println!("LIBP2P DISCOVERED PEER {:?}", peer);
                        libp2p::Swarm::dial(&mut self.swarm, peer);
                    }
                    MyBehaviourEvent::ExpiredPeer(_peer) => {}
                    MyBehaviourEvent::GossipMessage {
                        source,
                        topics,
                        message,
                    } => {
                        let message = String::from_utf8(message).unwrap();
                        println!("Received Gossip: {:?} {:?} {:?}", source, topics, message);
                        return Ok(Async::Ready(Option::from(NetworkEvent::PubsubMessage {
                            source,
                            topics,
                            message,
                        })));
                    }
                },
                Ok(Async::Ready(None)) => break,
                Ok(Async::NotReady) => {
                    if let Some(a) = Swarm::listeners(&self.swarm).next() {
                        println!("Listening on {:?}", a);
                    }
                    break;
                }
                _ => break,
            }
        }
        println!("Libp2p Not ready");
        Ok(Async::NotReady)
    }
}

#[derive(Clone)]
pub enum NetworkEvent {
    PubsubMessage {
        source: PeerId,
        topics: Vec<TopicHash>,
        message: String,
    },
}

fn build_transport(local_key: identity::Keypair) -> Boxed<(PeerId, StreamMuxerBox), Error> {
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
