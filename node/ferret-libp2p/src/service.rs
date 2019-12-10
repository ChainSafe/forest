use super::behaviour::{MyBehaviour, MyBehaviourEvent};
use super::config::Libp2pConfig;
use futures::{Async, Stream};
use libp2p::{
    core, core::muxing::StreamMuxerBox, core::nodes::Substream, core::transport::boxed::Boxed,
    gossipsub::TopicHash, identity, mplex, secio, yamux, PeerId, Swarm, Transport,
};
use slog::{debug, error, info, Logger};
use std::io::{Error, ErrorKind};
use std::time::Duration;
type Libp2pStream = Boxed<(PeerId, StreamMuxerBox), Error>;
type Libp2pBehaviour = MyBehaviour<Substream<StreamMuxerBox>>;

/// The Libp2pService listens to events from the Libp2p swarm.
pub struct Libp2pService {
    pub swarm: Swarm<Libp2pStream, Libp2pBehaviour>,
}

impl Libp2pService {
    /// Constructs a Libp2pService
    pub fn new(log: &Logger, config: &Libp2pConfig) -> Self {
        // TODO @Greg do local storage
        let local_key = identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());
        info!(log, "Local peer id: {:?}", local_peer_id);

        let transport = build_transport(local_key.clone());

        let mut swarm = {
            let be = MyBehaviour::new(log.clone(), &local_key);
            Swarm::new(transport, be, local_peer_id)
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

pub fn build_transport(local_key: identity::Keypair) -> Boxed<(PeerId, StreamMuxerBox), Error> {
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
