mod cli;

use cli::cli;

use libp2p::{
    gossipsub::Topic,
    tokio_codec::{FramedRead, LinesCodec},
};

use tokio::sync::mpsc;

use ferret_libp2p::config::Libp2pConfig;
use ferret_libp2p::service::NetworkEvent;
use network::service::*;

use futures::prelude::*;

use std::sync::Arc;

use tokio;

use tokio::runtime::Runtime;

fn main() {
    cli();

    /// TODO Everything below should be run in a function somewhere, but since we only have this main right now, should be ok to leave here
    let rt = Runtime::new().unwrap();

    let (tx, _rx) = mpsc::unbounded_channel::<NetworkEvent>();
    let tx = Arc::new(tx);
    let mut netcfg = Libp2pConfig::default();
    let topic = Topic::new("test-net".into());
    netcfg.pubsub_topics.push(topic.clone());

    let (network_service, mut net_tx, _exit_tx) = NetworkService::new(&netcfg, tx, &rt.executor());

    let _network_service = Arc::new(network_service);
    let stdin = tokio_stdin_stdout::stdin(0);
    let mut framed_stdin = FramedRead::new(stdin, LinesCodec::new());
    let _listening = false;

    rt.executor()
        .spawn(futures::future::poll_fn(move || -> Result<_, ()> {
            loop {
                match framed_stdin.poll().expect("Error while polling stdin") {
                    Async::Ready(Some(line)) => {
                        println!("Got msg from stdin");
                        net_tx.try_send(NetworkMessage::PubsubMessage {
                            topics: topic.clone(),
                            message: line.as_bytes().to_vec(),
                        })
                    }
                    Async::Ready(None) => panic!("Stdin closed"),
                    Async::NotReady => break,
                };
            }
            Ok(Async::NotReady)
        }));

    rt.shutdown_on_idle().wait().unwrap();
}
