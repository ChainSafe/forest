// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::peer_manager::PeerManager;
use async_std::prelude::*;
use async_std::sync::Mutex;
use async_std::sync::Receiver;
use async_std::task;
use flo_stream::{MessagePublisher, Publisher};
use forest_libp2p::rpc::{RPCResponse, RequestId};
use forest_libp2p::NetworkEvent;
use futures::channel::oneshot::Sender as OneShotSender;
use log::{debug, trace};
use std::collections::HashMap;
use std::sync::Arc;

/// Handles network events from channel and splits based on request
pub(crate) struct NetworkHandler {
    event_send: Publisher<NetworkEvent>,
    receiver: Receiver<NetworkEvent>,
    /// keeps track of a mapping from rpc request id to oneshot senders
    request_table: Arc<Mutex<HashMap<RequestId, OneShotSender<RPCResponse>>>>,
}

impl NetworkHandler {
    pub(crate) fn new(
        receiver: Receiver<NetworkEvent>,
        event_send: Publisher<NetworkEvent>,
        request_table: Arc<Mutex<HashMap<RequestId, OneShotSender<RPCResponse>>>>,
    ) -> Self {
        Self {
            receiver,
            event_send,
            request_table,
        }
    }

    pub(crate) fn spawn(&self, peer_manager: Arc<PeerManager>) {
        let mut receiver = self.receiver.clone();
        let mut event_send = self.event_send.republish();
        let request_table = self.request_table.clone();

        task::spawn(async move {
            loop {
                match receiver.next().await {
                    // Handle specifically RPC responses and send to that channel
                    Some(NetworkEvent::RPCResponse { req_id, response }) => {
                        // look up the request_table for the id and send through channel
                        let tx = request_table.lock().await.remove(&req_id);
                        if tx.is_none() {
                            debug!("RPCResponse receive failed: channel not found");
                            continue;
                        }
                        let tx = tx.unwrap();

                        match tx.send(response) {
                            Err(e) => debug!("RPCResponse receive failed: {:?}", e),
                            Ok(_) => {}
                        };
                    }
                    // Pass any non RPC responses through event channel
                    Some(event) => {
                        // Update peer on this thread before sending hello
                        if let NetworkEvent::Hello { source, .. } = &event {
                            // TODO should probably add peer with their tipset/ not handled seperately
                            peer_manager.add_peer(source.clone(), None).await;
                        }
                        if let NetworkEvent::BitswapBlock { .. } = &event {
                            event_send.publish(event).await
                        }
                    }
                    None => break,
                }
            }
        });
        trace!("Spawned network handler");
    }
}
