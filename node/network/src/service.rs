// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use ferret_libp2p::config::Libp2pConfig;
use ferret_libp2p::service::{Libp2pService, NetworkEvent};
use futures::stream::{Stream, };
use futures::channel::{mpsc, oneshot};
use futures::future::{Future, Select, FutureExt, TryFutureExt};
use futures::{select};
use async_std::task;
use libp2p::gossipsub::Topic;
use slog::{warn, Logger};
use std::sync::{Arc, Mutex};
use std::{error, task::Context, task::Poll};
use futures::prelude::*;


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
    ) -> (
        Self,
        mpsc::UnboundedSender<NetworkMessage>,
        oneshot::Sender<u8>,
    ) {
        let (tx, rx) = mpsc::unbounded();

        let libp2p_service = Arc::new(Mutex::new(Libp2pService::new(log, config)));

        let exit_tx = start(
            log.clone(),
            libp2p_service.clone(),
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
    outbound_transmitter: mpsc::UnboundedSender<NetworkEvent>,
    message_receiver: mpsc::UnboundedReceiver<NetworkMessage>,
) -> oneshot::Sender<u8> {
    let (network_exit, exit_rx) = oneshot::channel();
    task::spawn( async {
        poll(log, libp2p_service, outbound_transmitter, message_receiver).await;
    }
//        select(
//            poll(log, libp2p_service, outbound_transmitter, message_receiver),
//            select(exit_rx.then(|_| async {Ok(())}))
//        )
//            .then(move |_| Ok(()))
//        select!{
//            () = poll(log, libp2p_service, outbound_transmitter, message_receiver) =>{},
//            _ = exit_rx.then(|_| async {Ok(())}) => {}
//        }

    );

    network_exit
}

fn poll(
    log: Logger,
    libp2p_service: Arc<Mutex<Libp2pService>>,
    mut outbound_transmitter: mpsc::UnboundedSender<NetworkEvent>,
    mut message_receiver: mpsc::UnboundedReceiver<NetworkMessage>,
) -> impl futures::Future<Output = Result<(), Error>> {
    future::poll_fn(move |cx: &mut Context| {
        loop {
            match message_receiver.try_next() {
                Ok(Some(event))=> match event {
                    NetworkMessage::PubsubMessage { topics, message } => {
                        libp2p_service
                            .lock()
                            .unwrap()
                            .swarm
                            .publish(&topics, message);
                    }
                },
                _ => break,
            }
        }
        loop {
            match libp2p_service.lock().unwrap().poll_next_unpin(cx) {
                Poll::Ready(Some(event)) => match event {
                    NetworkEvent::PubsubMessage {
                        source,
                        topics,
                        message,
                    } => {
                        if outbound_transmitter
                            .unbounded_send(NetworkEvent::PubsubMessage {
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
                Poll::Ready(None) => unreachable!("Stream never ends"),
                Poll::Pending => break,
                _ => break,
            }
        }
        Poll::Pending
    })
}
