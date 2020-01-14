// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use ferret_libp2p::config::Libp2pConfig;
use ferret_libp2p::service::{Libp2pService, NetworkEvent};
use futures::stream::Stream;
use futures::{Async, Future};
use libp2p::gossipsub::Topic;
use slog::{warn, Logger};
use std::sync::{Arc, Mutex};
use tokio::runtime::TaskExecutor;
use tokio::sync::mpsc;

/// Ingress events to the NetworkService
pub enum NetworkMessage {
    PubsubMessage { topics: Topic, message: Vec<u8> },
}

/// Receives commands through a channel which communicates with Libp2p.
/// It also listens to the Libp2p service for messages.
pub struct NetworkService {
    /// Libp2p instance
    pub libp2p: Arc<Mutex<Libp2pService>>,
}

impl NetworkService {
    /// Starts a Libp2pService with a given config, UnboundedSender, and tokio executor.
    /// Returns an UnboundedSender channel so messages can come in.
    pub fn new(
        config: &Libp2pConfig,
        log: &Logger,
        outbound_transmitter: mpsc::UnboundedSender<NetworkEvent>,
        executor: &TaskExecutor,
    ) -> (
        Self,
        mpsc::UnboundedSender<NetworkMessage>,
        tokio::sync::oneshot::Sender<u8>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();

        let libp2p_service = Arc::new(Mutex::new(Libp2pService::new(log, config)));

        let exit_tx = start(
            log.clone(),
            libp2p_service.clone(),
            executor,
            outbound_transmitter,
            rx,
        );

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
    log: Logger,
    libp2p_service: Arc<Mutex<Libp2pService>>,
    executor: &TaskExecutor,
    outbound_transmitter: mpsc::UnboundedSender<NetworkEvent>,
    message_receiver: mpsc::UnboundedReceiver<NetworkMessage>,
) -> tokio::sync::oneshot::Sender<u8> {
    let (network_exit, exit_rx) = tokio::sync::oneshot::channel();
    executor.spawn(
        poll(log, libp2p_service, outbound_transmitter, message_receiver)
            .select(exit_rx.then(|_| Ok(())))
            .then(move |_| Ok(())),
    );

    network_exit
}

fn poll(
    log: Logger,
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
                        if outbound_transmitter
                            .try_send(NetworkEvent::PubsubMessage {
                                source,
                                topics,
                                message,
                            })
                            .is_err()
                        {
                            warn!(log, "Cant handle message");
                        }
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
