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

use tokio;

use tokio::runtime::Runtime;

fn main() {
    cli();

    // TODO Everything below should be run in a function somewhere, but since we only have this
    // main right now, should be ok to leave here
    // Create the tokio runtime
    let rt = Runtime::new().unwrap();

    // Create the channel so we can receive messages from NetworkService
    let (tx, mut rx) = mpsc::unbounded_channel::<NetworkEvent>();
    // Create the default libp2p config
    let mut netcfg = Libp2pConfig::default();
    let (_network_service, mut net_tx, _exit_tx) = NetworkService::new(&netcfg, tx, &rt.executor());

    let stdin = tokio_stdin_stdout::stdin(0);
    let mut framed_stdin = FramedRead::new(stdin, LinesCodec::new());
    let topics = netcfg.pubsub_topics.clone();

    rt.executor()
        .spawn(futures::future::poll_fn(move || -> Result<_, _> {
            loop {
                match framed_stdin.poll().expect("Error while polling stdin") {
                    Async::Ready(Some(line)) => net_tx.try_send(NetworkMessage::PubsubMessage {
                        topics: topics[0].clone(),
                        message: line.as_bytes().to_vec(),
                    }),
                    Async::Ready(None) => panic!("Stdin closed"),
                    Async::NotReady => break,
                };
            }
            loop {
                match rx.poll() {
                    Ok(Async::Ready(Some(message))) => match message {
                        NetworkEvent::PubsubMessage {
                            source,
                            topics,
                            message,
                        } => {
                            println!("Got msg! {:?} {:?} {:?}", source, topics, message);
                        }
                    },
                    _ => break,
                }
            }
            Ok(Async::NotReady)
        }));

    rt.shutdown_on_idle().wait().unwrap();
}
