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

use futures::sync::mpsc;

use ferret_libp2p::behaviour::*;
use ferret_libp2p::service::*;

use futures::prelude::*;

use env_logger::{Builder, Env};
use ferret_libp2p::service;
use std::sync::Arc;
use tokio::prelude::*;
use tokio;
use futures::future::lazy;
use tokio::runtime::current_thread::Runtime;

fn main(){
    Builder::from_env(Env::default().default_filter_or("info")).init();
    let (tx, rx) = mpsc::unbounded::<NetworkEvent>();
    let tx = Arc::new(tx);
    let (mut network_service, net_tx) = service::Service::new(tx.clone()).unwrap();
    let stdin = tokio_stdin_stdout::stdin(0);
    let mut framed_stdin = FramedRead::new(stdin, LinesCodec::new());
    let mut listening = false;

//    let mut rt = Runtime::new().unwrap();

    // tokio::runtime::run(lazy (|| {
    //     network_service.start();
    //     Ok(())
    // }));
//    rt.block_on(network_service.start());
    network_service.start();
    // tokio::run(futures::future::poll_fn(move || -> Result<_, ()> {
    //     loop {
    //         match framed_stdin.poll().expect("Error while polling stdin") {
    //             Async::Ready(Some(line)) => println!("aaa"),
    //             // Async::Ready(Some(line)) => network_service.publish(&topic, line.as_bytes()),
    //             Async::Ready(None) => panic!("Stdin closed"),
    //             Async::NotReady => break,
    //         };
    //     }
    //     Ok(Async::NotReady)
    // }).map_err(|e| println!("error = {:?}", e)));

}