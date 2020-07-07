// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::future;
use async_std::sync::{Mutex, Sender};
use blocks::{FullTipset, Tipset, TipsetKeys};
use flo_stream::Subscriber;
use forest_libp2p::{
    blocksync::{BlockSyncRequest, BlockSyncResponse, BLOCKS, MESSAGES},
    hello::HelloRequest,
    rpc::{RPCRequest, RPCResponse, RequestId},
    NetworkEvent, NetworkMessage,
};

use futures::channel::oneshot::{
    channel as oneshot_channel, Receiver as OneShotReceiver, Sender as OneShotSender,
};
use libp2p::core::PeerId;
use log::trace;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Timeout for response from an RPC request
const RPC_TIMEOUT: u64 = 5;

/// Context used in chain sync to handle network requests
pub struct SyncNetworkContext {
    /// Channel to send network messages through p2p service
    network_send: Sender<NetworkMessage>,

    /// Handles sequential request ID enumeration for requests
    request_id: RequestId,

    /// Receiver channel for network events
    pub receiver: Subscriber<NetworkEvent>,
    request_table: Arc<Mutex<HashMap<RequestId, OneShotSender<RPCResponse>>>>,
}

impl SyncNetworkContext {
    pub fn new(
        network_send: Sender<NetworkMessage>,
        receiver: Subscriber<NetworkEvent>,
        request_table: Arc<Mutex<HashMap<RequestId, OneShotSender<RPCResponse>>>>,
    ) -> Self {
        Self {
            network_send,
            receiver,
            request_table,
            request_id: RequestId(1),
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
            .await;

        // TODO: Handle Error
        if let RPCResponse::BlockSync(bs_res) = rpc_res.await.unwrap() {
            Ok(bs_res)
        } else {
            Err("Invalid response type".to_owned())
        }
    }

    /// Send a hello request to the network (does not await response)
    pub async fn hello_request(&mut self, peer_id: PeerId, request: HelloRequest) {
        trace!("Sending Hello Message {:?}", request);
        // TODO update to await response when we want to handle the latency
        self.network_send
            .send(NetworkMessage::RPC {
                peer_id,
                request: RPCRequest::Hello(request),
                id: self.request_id,
            })
            .await;
        self.request_id.0 += 1;
    }

    /// Send any RPC request to the network and await the response
    pub async fn send_rpc_request(
        &mut self,
        peer_id: PeerId,
        request: RPCRequest,
    ) -> OneShotReceiver<RPCResponse> {
        let request_id = self.request_id;
        self.request_id.0 += 1;

        let (tx, rx) = oneshot_channel();
        self.request_table.lock().await.insert(request_id, tx);
        self.network_send
            .send(NetworkMessage::RPC {
                peer_id,
                request,
                id: request_id,
            })
            .await;
        rx
    }
}
