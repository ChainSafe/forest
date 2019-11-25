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

use tokio::sync::mpsc;

use ferret_libp2p::behaviour::*;
use ferret_libp2p::service::{NetworkEvent};
use network::service::*;

use futures::prelude::*;

use env_logger::{Builder, Env};
use ferret_libp2p::service;
use std::sync::Arc;
use tokio::prelude::*;
use tokio;
use futures::future::lazy;
use tokio::runtime::Runtime;
use std::sync::Mutex;

fn main(){
    Builder::from_env(Env::default().default_filter_or("info")).init();


    let mut rt = Runtime::new().unwrap();

    let (tx, rx) = mpsc::unbounded_channel::<NetworkEvent>();
    let mut tx = Arc::new(tx);

    let (mut network_service,  mut net_tx, mut exit_tx) = NetworkService::new(tx.clone(),&rt.executor());


    // Reach out to another node if specified
    if let Some(to_dial) = std::env::args().nth(1) {
        let dialing = to_dial.clone();
        match to_dial.parse() {
            Ok(to_dial) => {
                match libp2p::Swarm::dial_addr(&mut network_service.libp2p.lock().unwrap().swarm, to_dial) {
                    Ok(_) => println!("Dialed {:?}", dialing),
                    Err(e) => println!("Dial {:?} failed: {:?}", dialing, e)
                }
            },
            Err(err) => println!("Failed to parse address to dial: {:?}", err),
        }
    }

    let network_service = Arc::new(network_service);
    let stdin = tokio_stdin_stdout::stdin(0);
    let mut framed_stdin = FramedRead::new(stdin, LinesCodec::new());
    let mut listening = false;

    let topic = Topic::new("test-net".into());

    println!("Polling for stdin");
    rt.executor().spawn(futures::future::poll_fn(move || -> Result<_, ()> {
        loop {
            match framed_stdin.poll().expect("Error while polling stdin") {
                Async::Ready(Some(line)) => {
                    println!("Got msg from stdin");
                    net_tx.try_send(
                    NetworkMessage::PubsubMessage {
                        topics: topic.clone(),
                        message: line.as_bytes().to_vec()
                    })
                },
                Async::Ready(None) => panic!("Stdin closed"),
                Async::NotReady => break,
            };
        }
        Ok(Async::NotReady)
    }));
    rt.shutdown_on_idle()
        .wait().unwrap();
}