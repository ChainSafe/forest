use ferret_libp2p::config::Libp2pConfig;
use ferret_libp2p::service::{Libp2pService, NetworkEvent};
use futures::stream::Stream;
use futures::Async;
use futures::Future;
use libp2p::{gossipsub::Topic};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use tokio::runtime::TaskExecutor;

/// Ingress events to the NetworkService
pub enum NetworkMessage {
    PubsubMessage { topics: Topic, message: Vec<u8> },
}

/// The NetworkService receives commands through a channel which communicates with Libp2p.
/// It also listens to the Libp2p service for
pub struct NetworkService {
    pub libp2p: Arc<Mutex<Libp2pService>>,
}

impl NetworkService {
    /// Starts a Libp2pService with a given config, UnboundedSender, and tokio executor.
    /// Returns an UnboundedSender channel so messages can come in.
    ///
    /// # Example
    /// ```
    /// use tokio::runtime::Runtime;
    /// use tokio::sync::mpsc;
    /// use ferret_libp2p::service::NetworkEvent;
    /// use ferret_libp2p::config::Libp2pConfig;
    /// use std::sync::Arc;
    ///
    ///
    /// let rt = Runtime::new().unwrap();
    /// let (tx, _rx) = mpsc::unbounded_channel::<NetworkEvent>();
    /// let tx = Arc::new(tx);
    /// let mut netcfg = Libp2pConfig::default();
    /// let (network_service, mut net_tx, _exit_tx) = new(&netcfg, tx, &rt.executor());
    /// ```
    pub fn new(
        config: &Libp2pConfig,
        outbound_transmitter: mpsc::UnboundedSender<NetworkEvent>,
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

enum Error {}

/// Spawns the NetworkService service.
fn start(
    libp2p_service: Arc<Mutex<Libp2pService>>,
    executor: &TaskExecutor,
    outbound_transmitter: mpsc::UnboundedSender<NetworkEvent>,
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
    mut outbound_transmitter: mpsc::UnboundedSender<NetworkEvent>,
    mut message_receiver: mpsc::UnboundedReceiver<NetworkMessage>,
) -> impl futures::Future<Item = (), Error = Error> {
    futures::future::poll_fn(move || -> Result<_, _> {
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
                       outbound_transmitter.try_send(NetworkEvent::PubsubMessage{
                           source, topics, message
                       });
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
