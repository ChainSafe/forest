use libp2p::{
    identity,
    PeerId,
    gossipsub::{
        Topic,
    },
    swarm::{
        Swarm,
    },
    tokio_codec::{FramedRead, LinesCodec},
};
use tokio;


use ferret_libp2p::behaviour::*;

use futures::prelude::*;

use env_logger::{Builder, Env};


fn main(){
    Builder::from_env(Env::default().default_filter_or("info")).init();
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    println!("Local peer id: {:?}", local_peer_id);

    // Set up an encrypted TCP Transport over the Mplex and Yamux protocols
    let transport = libp2p::build_development_transport(local_key.clone());

    // Create a Floodsub/Gossipsub topic
    let topic = Topic::new("test-net".into());

    let mut swarm = {
        let be = MyBehaviour::new(&local_key);
        Swarm::new(transport, be, local_peer_id)
    };
    swarm.gossipsub.subscribe(topic.clone());

    if let Some(x) = std::env::args().nth(1) {
        println!("Hello, world! {}", x);
    } else {
        println!("Nothing");
    }

    Swarm::listen_on(&mut swarm, "/ip4/0.0.0.0/tcp/0".parse().unwrap()).unwrap();

    let stdin = tokio_stdin_stdout::stdin(0);
    let mut framed_stdin = FramedRead::new(stdin, LinesCodec::new());
    let mut listening = false;
    tokio::run(futures::future::poll_fn(move || -> Result<_, ()> {
        loop {
            match framed_stdin.poll().expect("Error while polling stdin") {
                Async::Ready(Some(line)) => swarm.publish(&topic, line.as_bytes()),
                Async::Ready(None) => panic!("Stdin closed"),
                Async::NotReady => break,
            };
        }
        loop {
            match swarm.poll() {
                Ok(Async::Ready(Some(event))) => match event {
                    MyBehaviourEvent::DiscoveredPeer(peer) => {
                        libp2p::Swarm::dial(&mut swarm, peer);
                    },
                    MyBehaviourEvent::ExpiredPeer(peer) => {
                    },
                    MyBehaviourEvent::GossipMessage {
                        source,
                        topics,
                        message,
                    } => {
                        println!("Received Gossip: {:?} {:?} {:?}", source, topics, String::from_utf8(message).unwrap());
                    }

                },
                Ok(Async::Ready(None)) | Ok(Async::NotReady) => {
                    if !listening {
                        if let Some(a) = Swarm::listeners(&swarm).next() {
                            println!("Listening on {:?}", a);
                            listening = true;
                        }
                    }
                    break
                },
                _ => {}
            }
        }
        Ok(Async::NotReady)
    }));
}