// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::peer_manager::PeerManager;
use async_std::prelude::*;
use async_std::sync::Mutex;
use async_std::sync::{Receiver, Sender};
use async_std::task;
use forest_libp2p::rpc::{RPCResponse, RequestId};
use forest_libp2p::NetworkEvent;
use futures::channel::oneshot::{
  Sender as OneShotSender,
};
use log::trace;
use std::collections::HashMap;
use std::sync::Arc;
use flo_stream::{MessagePublisher, Publisher};

pub(crate) type RPCSender = Sender<(RequestId, RPCResponse)>;

/// Handles network events from channel and splits based on request
pub(crate) struct NetworkHandler {
    rpc_send: RPCSender,
    event_send: Publisher<NetworkEvent>,
    receiver: Receiver<NetworkEvent>,
    request_table: Arc<Mutex<HashMap<usize, OneShotSender<RPCResponse>>>>,
}

impl NetworkHandler {
    pub(crate) fn new(
        receiver: Receiver<NetworkEvent>,
        rpc_send: RPCSender,
        event_send: Publisher<NetworkEvent>,
        request_table: Arc<Mutex<HashMap<usize, OneShotSender<RPCResponse>>>>,
    ) -> Self {
        Self {
            receiver,
            rpc_send,
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
                        let tx = request_table.lock().await.remove(&req_id).unwrap();
                        tx.send(response).unwrap();
                    }
                    // Pass any non RPC responses through event channel
                    Some(event) => {
                        // Update peer on this thread before sending hello
                        if let NetworkEvent::Hello { source, .. } = &event {
                            // TODO should probably add peer with their tipset/ not handled seperately
                            peer_manager.add_peer(source.clone(), None).await;
                        }

                        event_send.publish(event).await
                    }
                    None => break,
                }
            }
        });
        trace!("Spawned network handler");
    }
}
