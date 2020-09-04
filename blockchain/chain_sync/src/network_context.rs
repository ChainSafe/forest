// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::future;
use async_std::sync::Sender;
use blocks::{FullTipset, Tipset, TipsetKeys};
use flo_stream::Subscriber;
use forest_libp2p::{
    blocksync::{BlockSyncRequest, BlockSyncResponse, BLOCKS, MESSAGES},
    hello::HelloRequest,
    NetworkEvent, NetworkMessage,
};
use futures::channel::oneshot::channel as oneshot_channel;
use libp2p::core::PeerId;
use log::trace;
use std::time::Duration;

/// Timeout for response from an RPC request
const RPC_TIMEOUT: u64 = 20;

/// Context used in chain sync to handle network requests
pub struct SyncNetworkContext {
    /// Channel to send network messages through p2p service
    network_send: Sender<NetworkMessage>,

    /// Receiver channel for network events
    pub receiver: Subscriber<NetworkEvent>,
}

impl SyncNetworkContext {
    pub fn new(network_send: Sender<NetworkMessage>, receiver: Subscriber<NetworkEvent>) -> Self {
        Self {
            network_send,
            receiver,
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

        let ts: Vec<Tipset> = bs_res.into_result()?;
        Ok(ts)
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

        let (tx, rx) = oneshot_channel();
        self.network_send
            .send(NetworkMessage::BlockSyncRequest {
                peer_id,
                request,
                response_channel: tx,
            })
            .await;

        match future::timeout(Duration::from_secs(RPC_TIMEOUT), rx).await {
            Ok(Ok(bs_res)) => Ok(bs_res),
            Ok(Err(e)) => Err(format!("RPC error: {}", e.to_string())),
            Err(_) => Err("Connection timed out".to_string()),
        }
    }

    /// Send a hello request to the network (does not await response)
    pub async fn hello_request(&mut self, peer_id: PeerId, request: HelloRequest) {
        trace!("Sending Hello Message {:?}", request);
        // TODO update to await response when we want to handle the latency
        self.network_send
            .send(NetworkMessage::HelloRequest { peer_id, request })
            .await;
    }
}
