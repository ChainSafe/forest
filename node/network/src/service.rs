use ferret_libp2p::service::{Libp2pService, NetworkEvent};
use tokio::sync::mpsc;

use std::sync::{Arc, Mutex};
//use std::error::Error;
use libp2p::{self, Swarm, core::transport::boxed::Boxed, core, secio, yamux, mplex, PeerId, core::muxing::StreamMuxerBox, core::nodes::Substream, identity, gossipsub::{Topic, TopicHash}, Transport, build_development_transport};
use tokio::runtime::TaskExecutor;
use futures::Async;
use futures::stream::Stream;
use futures::Future;


pub enum NetworkMessage {
    PubsubMessage {
        topics: Topic,
        message: Vec<u8>,
    },
}

pub struct NetworkService{
    pub libp2p: Arc<Mutex<Libp2pService>>,
}

impl NetworkService{
    pub fn new (
        outbound_transmitter: Arc<mpsc::UnboundedSender<NetworkEvent>>,
        executor: &TaskExecutor,
    ) -> (Self, mpsc::UnboundedSender<NetworkMessage>, tokio::sync::oneshot::Sender<u8>) {
        let (tx, rx) = mpsc::unbounded_channel();

        let libp2p_service = Arc::new(Mutex::new(Libp2pService::new().unwrap()));

        let exit_tx = start(libp2p_service.clone(), executor, outbound_transmitter, rx);

        return (NetworkService{
            libp2p: libp2p_service,
        }, tx, exit_tx);
    }
}

enum Error{
    aaa (u8)
}



pub fn start (
    libp2p_service: Arc<Mutex<Libp2pService>>,
    executor: &TaskExecutor,
    outbound_transmitter: Arc<mpsc::UnboundedSender<NetworkEvent>>,
    mut message_receiver: mpsc::UnboundedReceiver<NetworkMessage>,
) -> tokio::sync::oneshot::Sender<u8> {
    let (network_exit, exit_rx) = tokio::sync::oneshot::channel();
    executor.spawn(
        poll(libp2p_service,outbound_transmitter,message_receiver)
            .select(exit_rx.then(|_| Ok(())))
            .then(move |_| {
                Ok(())
            }),
    );

    network_exit
}

fn poll(
    libp2p_service: Arc<Mutex<Libp2pService>>,
    outbound_transmitter: Arc<mpsc::UnboundedSender<NetworkEvent>>,
    mut message_receiver: mpsc::UnboundedReceiver<NetworkMessage>,
) -> impl futures::Future<Item = (), Error = Error> {
    futures::future::poll_fn(move || -> Result<_, Error> {
        loop {
            match message_receiver.poll() {
                Ok(Async::Ready(Some(event))) => match event {
                    NetworkMessage::PubsubMessage {
                        topics,
                        message
                    } => {
                        println!("Got a msg from msgchannel");
                        libp2p_service.lock().unwrap().swarm.publish(&topics, message);
                    }
                },
                Ok(Async::NotReady) => break,
                _ => {break}
            }
            match libp2p_service.lock().unwrap().poll() {
                Ok(Async::Ready(Some(event))) => match event {
                    NetworkEvent::PubsubMessage { source, topics, message } => {
                        println!("ASDFASDFSADFSAF");
                    }
                }
                Ok(Async::Ready(None)) => unreachable!("Stream never ends"),
                Ok(Async::NotReady) => break,
                _ => {break}
            }
        }
        Ok(Async::NotReady)
    })
}