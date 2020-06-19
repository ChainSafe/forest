// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::network_handler::RPCReceiver;
use async_std::prelude::*;
use async_std::sync::{Receiver, Sender, Mutex};
use blocks::{FullTipset, Tipset, TipsetKeys};
use forest_libp2p::{
    blocksync::{BlockSyncRequest, BlockSyncResponse, BLOCKS, MESSAGES},
    hello::HelloMessage,
    rpc::{RPCEvent, RPCRequest, RPCResponse, RequestId},
    NetworkEvent, NetworkMessage,
};
use futures::channel::oneshot::{channel as oneshot_channel, Receiver as OneShotReceiver, Sender as OneShotSender};
use libp2p::core::PeerId;
use log::trace;
use std::time::Duration;
use std::sync::Arc;
use std::collections::HashMap;

/// Timeout for response from an RPC request
const RPC_TIMEOUT: u64 = 5;

/// Context used in chain sync to handle network requests
pub struct SyncNetworkContext {
    /// Channel to send network messages through p2p service
    network_send: Sender<NetworkMessage>,

    /// Handles sequential request ID enumeration for requests
    rpc_request_id: RequestId,

    /// Receiver channel for BlockSync responses
    rpc_receiver: RPCReceiver,

    /// Receiver channel for network events
    pub receiver: Receiver<NetworkEvent>,
    request_table: Arc<Mutex<HashMap<RequestId, OneShotSender<RPCResponse>>>>,

}

impl SyncNetworkContext {
    pub fn new(
        network_send: Sender<NetworkMessage>,
        rpc_receiver: RPCReceiver,
        receiver: Receiver<NetworkEvent>,
        request_table: Arc<Mutex<HashMap<RequestId, OneShotSender<RPCResponse>>>>,
    ) -> Self {
        Self {
            network_send,
            rpc_receiver,
            receiver,
            rpc_request_id: 1,
            request_table,
        }
    }

    /// Send a blocksync request for only block headers (ignore messages)
    pub async fn blocksync_headers(
        &mut self,
        peer_id: PeerId,
        tsk: &TipsetKeys,
        count: u64,
    ) -> Result<Vec<Tipset>, String> {
        let bs_res = self
            .blocksync_request(
                peer_id,
                BlockSyncRequest {
                    start: tsk.cids().to_vec(),
                    request_len: count,
                    options: BLOCKS,
                },
            )
            .await?;

        let ts = bs_res.into_result()?;
        Ok(ts.iter().map(|fts| fts.to_tipset()).collect())
    }
    /// Send a blocksync request for full tipsets (includes messages)
    pub async fn blocksync_fts(
        &mut self,
        peer_id: PeerId,
        tsk: &TipsetKeys,
    ) -> Result<FullTipset, String> {
        let bs_res = self
            .blocksync_request(
                peer_id,
                BlockSyncRequest {
                    start: tsk.cids().to_vec(),
                    request_len: 1,
                    options: BLOCKS | MESSAGES,
                },
            )
            .await?;

        let fts = bs_res.into_result()?;
        fts.get(0)
            .cloned()
            .ok_or(format!("No full tipset found for cid: {:?}", tsk))
    }

    /// Send a blocksync request to the network and await response
    pub async fn blocksync_request(
        &mut self,
        peer_id: PeerId,
        request: BlockSyncRequest,
    ) -> Result<BlockSyncResponse, String> {
        trace!("Sending BlockSync Request {:?}", request);
        let rpc_res = self
            .send_rpc_request(peer_id, RPCRequest::BlockSync(request))
            .await?;

        if let RPCResponse::BlockSync(bs_res) = rpc_res {
            Ok(bs_res)
        } else {
            Err("Invalid response type".to_owned())
        }
    }

    /// Send a hello request to the network (does not await response)
    pub async fn hello_request(&self, peer_id: PeerId, request: HelloMessage) {
        trace!("Sending Hello Message {:?}", request);
        // TODO update to await response when we want to handle the latency
        self.network_send.send(NetworkMessage::RPC{peer_id,  event: RPCEvent::Request(0, RPCRequest::Hello(request))})
            .await;
    }

    /// Send any RPC request to the network and await the response
    pub async fn send_rpc_request(
        &mut self,
        peer_id: PeerId,
        rpc_request: RPCRequest,
    ) -> Result<RPCResponse, String> {
        let request_id = self.rpc_request_id;
        self.rpc_request_id += 1;
        let rx = self.send_rpc_event(request_id, peer_id, RPCEvent::Request(request_id, rpc_request)).await;
        rx.await.map_err(|e| e.to_string())
    }

    /// Handles sending the base event to the network service
    async fn send_rpc_event(&self, req_id: RequestId, peer_id: PeerId, event: RPCEvent) -> OneShotReceiver<RPCResponse>{
        let (tx, rx) = oneshot_channel();
        self.request_table.lock().await.insert(req_id, tx);
        self.network_send
            .send(NetworkMessage::RPC { peer_id, event }).await;
        rx
    }
}
