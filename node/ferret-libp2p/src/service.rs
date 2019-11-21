use crate::behaviour::{MyBehaviour, MyBehaviourEvent};
use libp2p::{self, Swarm, core::transport::boxed::Boxed, core, secio, yamux, mplex, PeerId, core::muxing::StreamMuxerBox, core::nodes::Substream, identity, gossipsub::{Topic, TopicHash}, Transport, build_development_transport};
use futures::{Stream, Async};
use std::io::{Error, ErrorKind};
use std::time::Duration;
use futures::sync::mpsc;
use futures::future::Future;

use std::sync::Arc;

type Libp2pStream = Boxed<(PeerId, StreamMuxerBox), Error>;
type Libp2pBehaviour = MyBehaviour<Substream<StreamMuxerBox>>;

pub struct Service {
    pub swarm: Swarm<Libp2pStream, Libp2pBehaviour>,
    network_receiver: mpsc::UnboundedReceiver<NetworkMessage>,
    outbound_transmitter: Arc<mpsc::UnboundedSender<NetworkEvent>>,
}

impl Service {
    pub fn new (outbound_transmitter: Arc<mpsc::UnboundedSender<NetworkEvent>>) -> Result<(Self, Arc<mpsc::UnboundedSender<NetworkMessage>>), Error>
     {
        // Starting Libp2p Service

        // TODO @Greg do local storage
        let local_key = identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());


        let transport = build_transport(local_key.clone());
//        let transport = build_development_transport(&local_key);
        let mut swarm = {
            let be = MyBehaviour::new(&local_key);
            Swarm::new(transport, be, local_peer_id)
        };

        // TODO be able to specify port aand listening addr with proper error handling
        Swarm::listen_on(&mut swarm, "/ip4/0.0.0.0/tcp/0".parse().unwrap()).unwrap();

        // TODO be able to bootstrap peers
        // TODO build list of topics

        let topic = Topic::new("test-net".into());
        swarm.subscribe(topic.clone());
        let (tx, rx) = mpsc::unbounded();
        let tx = Arc::new(tx);
        Ok((Service{
            swarm: swarm,
            network_receiver: rx,
            outbound_transmitter
        }, tx.clone()))
    }
}

pub enum NetworkEvent {
    PubsubMessage {
        source: PeerId,
        topics: Vec<TopicHash>,
        message: Vec<u8>,
    },
}

pub enum NetworkMessage {
    PubsubMessage{
        topic: Topic,
        message: Vec<u8>,
    }
}

impl Service {
    pub  fn start(&mut self) {
        loop {
            match self.swarm.poll() {
                Ok(Async::Ready(Some(event))) => match event {
                    MyBehaviourEvent::DiscoveredPeer(peer) => {
                        libp2p::Swarm::dial(&mut self.swarm, peer);
                    },
                    MyBehaviourEvent::ExpiredPeer(peer) => {
                    },
                    MyBehaviourEvent::GossipMessage {
                        source,
                        topics,
                        message,
                    } => {
                        // TODO proper error handling
                        self.outbound_transmitter.unbounded_send(NetworkEvent::PubsubMessage {
                            source,
                            topics,
                            message,
                        }).unwrap_or_else(|e| {
                            panic!(
                                "failed to send in network_transmitter"
                            );
                        });
                    }
                },
                Ok(Async::Ready(None)) => {}
                Ok(Async::NotReady) => {},
                _ => {}
            }
        }
    }
}


fn build_transport(local_key: identity::Keypair) -> Boxed<(PeerId, StreamMuxerBox), Error> {
    let transport = libp2p::tcp::TcpConfig::new().nodelay(true);
    let transport = libp2p::dns::DnsConfig::new(transport);

    transport.upgrade(core::upgrade::Version::V1)
        .authenticate(secio::SecioConfig::new(local_key))
        .multiplex(core::upgrade::SelectUpgrade::new(yamux::Config::default(), mplex::MplexConfig::new()))
        .map(|(peer, muxer), _| (peer, core::muxing::StreamMuxerBox::new(muxer)))
        .timeout(Duration::from_secs(20))
        .map_err(|err| Error::new(ErrorKind::Other, err))
        .boxed()
}



//tokio::run(futures::future::poll_fn(move || -> Result<_, ()> {
//loop {
//match framed_stdin.poll().expect("Error while polling stdin") {
//Async::Ready(Some(line)) => swarm.publish(&topic, line.as_bytes()),
//Async::Ready(None) => panic!("Stdin closed"),
//Async::NotReady => break,
//};
//}
//loop {
//match swarm.poll() {
//Ok(Async::Ready(Some(event))) => match event {
//MyBehaviourEvent::DiscoveredPeer(peer) => {
//libp2p::Swarm::dial(&mut swarm, peer);
//},
//MyBehaviourEvent::ExpiredPeer(peer) => {
//},
//MyBehaviourEvent::GossipMessage {
//source,
//topics,
//message,
//} => {
//println!("Received Gossip: {:?} {:?} {:?}", source, topics, String::from_utf8(message).unwrap());
//}
//
//},
//Ok(Async::Ready(None)) | Ok(Async::NotReady) => {
//if !listening {
//if let Some(a) = Swarm::listeners(&swarm).next() {
//println!("Listening on {:?}", a);
//listening = true;
//}
//}
//break
//},
//_ => {}
//}
//}
//Ok(Async::NotReady)
//}));