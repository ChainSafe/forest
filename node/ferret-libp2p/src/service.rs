use crate::behaviour::{MyBehaviour, MyBehaviourEvent};
use libp2p::{self, Swarm, core::transport::boxed::Boxed, core, secio, yamux, mplex, PeerId, core::muxing::StreamMuxerBox, core::nodes::Substream, identity, gossipsub::{Topic, TopicHash}, Transport, build_development_transport};
use futures::{Stream, Async, Future};
use std::io::{Error, ErrorKind};
use std::time::Duration;
use futures::sync::mpsc;
use tokio::runtime::TaskExecutor;

use std::sync::{Arc, Mutex};
use futures::future::PollFn;
use futures::task::Spawn;
use futures::Lazy;
use futures::IntoFuture;
use std::ops::{Deref, DerefMut};

type Libp2pStream = Boxed<(PeerId, StreamMuxerBox), Error>;
type Libp2pBehaviour = MyBehaviour<Substream<StreamMuxerBox>>;

pub struct Libp2pService{
    pub swarm: Swarm<Libp2pStream, Libp2pBehaviour>,
}

impl Libp2pService{
    // TODO Allow bootstrap and topics
    pub fn new () -> Result<Self, Error>
    {
        // Starting Libp2p Service

        // TODO @Greg do local storage
        let local_key = identity::Keypair::generate_ed25519();
        let local_peer_id = PeerId::from(local_key.public());

        println!("Local peer id: {:?}", local_peer_id);

        let transport = build_transport(local_key.clone());
//        let transport = build_development_transport(local_key.clone())
//            .map_err(|err| Error::new(ErrorKind::Other, err))
//            .boxed();
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

        Ok((Libp2pService{
            swarm: swarm,
        }))
    }
}

impl Stream for Libp2pService {

    type Item = NetworkEvent;
    type Error = ();

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        let mut listening = false;
        loop {
            println!("loop poll");
            match self.swarm.poll() {
                Ok(Async::Ready(Some(event))) => match event {
                    MyBehaviourEvent::DiscoveredPeer(peer) => {
                        println!("LIBP2P DISCOVERED PEER {:?}", peer);
                        libp2p::Swarm::dial(&mut self.swarm, peer);
                    },
                    MyBehaviourEvent::ExpiredPeer(peer) => {
                    },
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
                           message
                        })));
                    }

                },
                Ok(Async::Ready(None)) => break,
                Ok(Async::NotReady) => {
                    if let Some(a) = Swarm::listeners(&self.swarm).next() {
                        println!("Listening on {:?}", a);
                    }
                    break;
                },
                _ => {break}
            }
        }
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

    transport.upgrade(core::upgrade::Version::V1)
        .authenticate(secio::SecioConfig::new(local_key))
        .multiplex(core::upgrade::SelectUpgrade::new(yamux::Config::default(), mplex::MplexConfig::new()))
        .map(|(peer, muxer), _| (peer, core::muxing::StreamMuxerBox::new(muxer)))
        .timeout(Duration::from_secs(20))
        .map_err(|err| Error::new(ErrorKind::Other, err))
        .boxed()
}



