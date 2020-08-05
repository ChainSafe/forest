// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::peer_manager::PeerManager;
use async_std::prelude::*;
use async_std::sync::Receiver;
use async_std::task;
use flo_stream::{MessagePublisher, Publisher};
use forest_libp2p::hello::HelloResponse;
use forest_libp2p::NetworkEvent;
use log::trace;
use std::sync::Arc;

/// Handles network events from channel and splits based on request
pub(crate) struct NetworkHandler {
    event_send: Publisher<NetworkEvent>,
    receiver: Receiver<NetworkEvent>,
}

impl NetworkHandler {
    pub(crate) fn new(
        receiver: Receiver<NetworkEvent>,
        event_send: Publisher<NetworkEvent>,
    ) -> Self {
        Self {
            receiver,
            event_send,
        }
    }

    pub(crate) fn spawn(&self, peer_manager: Arc<PeerManager>) {
        let mut receiver = self.receiver.clone();
        let mut event_send = self.event_send.republish();
        task::spawn(async move {
            while let Some(event) = receiver.next().await {
                // Update peer on this thread before sending hello
                if let NetworkEvent::HelloRequest { channel, .. } = &event {
                    // TODO should probably add peer with their tipset/ not handled seperately
                    channel
                        .clone()
                        .send(HelloResponse {
                            arrival: 100,
                            sent: 101,
                        })
                        .await;
                    peer_manager.add_peer(channel.peer.clone(), None).await;
                }
                event_send.publish(event).await
            }
        });
        trace!("Spawned network handler");
    }
}
