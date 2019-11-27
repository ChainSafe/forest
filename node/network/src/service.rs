use ferret_libp2p::service::{Libp2pService, NetworkEvent};
use ferret_libp2p::config::Libp2pConfig;
use tokio::sync::mpsc;
use std::sync::{Arc, Mutex};
use futures::stream::Stream;
use futures::Async;
use futures::Future;
use libp2p::{
    self,
    gossipsub::{Topic, },
};

use tokio::runtime::TaskExecutor;

pub enum NetworkMessage {
    PubsubMessage { topics: Topic, message: Vec<u8> },
}

pub struct NetworkService {
    pub libp2p: Arc<Mutex<Libp2pService>>,
}

impl NetworkService {
    pub fn new(
        config: &Libp2pConfig,
        outbound_transmitter: Arc<mpsc::UnboundedSender<NetworkEvent>>,
        executor: &TaskExecutor,
    ) -> (
        Self,
        mpsc::UnboundedSender<NetworkMessage>,
        tokio::sync::oneshot::Sender<u8>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();

        let libp2p_service = Arc::new(Mutex::new(Libp2pService::new(config).unwrap()));

        let exit_tx = start(libp2p_service.clone(), executor, outbound_transmitter, rx);

        (
            NetworkService {
                libp2p: libp2p_service,
            },
            tx,
            exit_tx,
        )
    }
}

enum Error {
}

pub fn start(
    libp2p_service: Arc<Mutex<Libp2pService>>,
    executor: &TaskExecutor,
    outbound_transmitter: Arc<mpsc::UnboundedSender<NetworkEvent>>,
    message_receiver: mpsc::UnboundedReceiver<NetworkMessage>,
) -> tokio::sync::oneshot::Sender<u8> {
    let (network_exit, exit_rx) = tokio::sync::oneshot::channel();
    executor.spawn(
        poll(libp2p_service, outbound_transmitter, message_receiver)
            .select(exit_rx.then(|_| Ok(())))
            .then(move |_| Ok(())),
    );

    network_exit
}

fn poll(
    libp2p_service: Arc<Mutex<Libp2pService>>,
    _outbound_transmitter: Arc<mpsc::UnboundedSender<NetworkEvent>>,
    mut message_receiver: mpsc::UnboundedReceiver<NetworkMessage>,
) -> impl futures::Future<Item = (), Error = Error> {
    futures::future::poll_fn(move || -> Result<_, Error> {
        loop {
            match message_receiver.poll() {
                Ok(Async::Ready(Some(event))) => match event {
                    NetworkMessage::PubsubMessage { topics, message } => {
                        libp2p_service
                            .lock()
                            .unwrap()
                            .swarm
                            .publish(&topics, message);
                    }
                },
                Ok(Async::NotReady) => break,
                _ => break,
            }
        }
        loop {
            match libp2p_service.lock().unwrap().poll() {
                Ok(Async::Ready(Some(event))) => match event {
                    NetworkEvent::PubsubMessage {
                        source,
                        topics,
                        message,
                    } => {
                        println!("Received a message from GossipSub! {:?}, {:?}, {:?}", source, topics, message);
                    }
                },
                Ok(Async::Ready(None)) => unreachable!("Stream never ends"),
                Ok(Async::NotReady) => break,
                _ => break,
            }
        }
        Ok(Async::NotReady)
    })
}
