// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::peer_manager::PeerManager;
use async_std::future;
use async_std::sync::Sender;
use blocks::{FullTipset, Tipset, TipsetKeys};
use forest_libp2p::{
    blocksync::{BlockSyncRequest, BlockSyncResponse, CompactedMessages, BLOCKS, MESSAGES},
    hello::HelloRequest,
    NetworkMessage,
};
use futures::channel::oneshot::channel as oneshot_channel;
use libp2p::core::PeerId;
use log::trace;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Timeout for response from an RPC request
const RPC_TIMEOUT: u64 = 20;

/// Context used in chain sync to handle network requests
#[derive(Clone)]
pub struct SyncNetworkContext {
    /// Channel to send network messages through p2p service
    network_send: Sender<NetworkMessage>,

    /// Manages peers to send requests to and updates request stats for the respective peers.
    peer_manager: Arc<PeerManager>,
}

impl SyncNetworkContext {
    pub fn new(network_send: Sender<NetworkMessage>, peer_manager: Arc<PeerManager>) -> Self {
        Self {
            network_send,
            peer_manager,
        }
    }

    /// Returns a reference to the peer manager of the network context.
    pub fn peer_manager(&self) -> &PeerManager {
        self.peer_manager.as_ref()
    }

    /// Clones the `Arc` to the peer manager.
    pub fn peer_manager_cloned(&self) -> Arc<PeerManager> {
        self.peer_manager.clone()
    }

    /// Send a blocksync request for only block headers (ignore messages)
    pub async fn blocksync_headers(
        &self,
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
    /// Send a blocksync request for only messages (ignore block headers).
    pub async fn blocksync_messages(
        &self,
        peer_id: PeerId,
        tsk: &TipsetKeys,
        count: u64,
    ) -> Result<Vec<CompactedMessages>, String> {
        let bs_res = self
            .blocksync_request(
                peer_id,
                BlockSyncRequest {
                    start: tsk.cids().to_vec(),
                    request_len: count,
                    options: MESSAGES,
                },
            )
            .await?;

        let ts: Vec<CompactedMessages> = bs_res.into_result()?;
        Ok(ts)
    }

    /// Send a blocksync request for a single full tipset (includes messages)
    pub async fn blocksync_fts(
        &self,
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

        let mut fts = bs_res.into_result()?;
        if fts.len() != 1 {
            return Err(format!(
                "Full tipset request returned {} tipsets",
                fts.len()
            ));
        }
        Ok(fts.remove(0))
    }

    async fn handle_blocksync_request(
        &self,
        peer_id: Option<PeerId>,
        tsk: &TipsetKeys,
        count: u64,
        options: u64,
    ) -> Result<BlockSyncResponse, String> {
        // let bs_res = self
        //     .blocksync_request(
        //         peer_id,
        //         BlockSyncRequest {
        //             start: tsk.cids().to_vec(),
        //             request_len: count,
        //             options,
        //         },
        //     )
        //     .await?;

        // let ts: Vec<CompactedMessages> = bs_res.into_result()?;
        todo!()
    }

    /// Send a blocksync request to the network and await response
    async fn blocksync_request(
        &self,
        peer_id: PeerId,
        request: BlockSyncRequest,
    ) -> Result<BlockSyncResponse, String> {
        trace!("Sending BlockSync Request {:?}", request);

        let req_pre_time = SystemTime::now();

        let (tx, rx) = oneshot_channel();
        self.network_send
            .send(NetworkMessage::BlockSyncRequest {
                peer_id: peer_id.clone(),
                request,
                response_channel: tx,
            })
            .await;

        match future::timeout(Duration::from_secs(RPC_TIMEOUT), rx).await {
            Ok(Ok(bs_res)) => {
                self.peer_manager
                    .log_success(
                        &peer_id,
                        SystemTime::now()
                            .duration_since(req_pre_time)
                            .unwrap_or_default(),
                    )
                    .await;
                Ok(bs_res)
            }
            Ok(Err(e)) => {
                self.peer_manager
                    .log_failure(
                        &peer_id,
                        SystemTime::now()
                            .duration_since(req_pre_time)
                            .unwrap_or_default(),
                    )
                    .await;
                Err(format!("RPC error: {}", e.to_string()))
            }
            Err(_) => {
                self.peer_manager
                    .log_failure(
                        &peer_id,
                        SystemTime::now()
                            .duration_since(req_pre_time)
                            .unwrap_or_default(),
                    )
                    .await;
                Err("Connection timed out".to_string())
            }
        }
    }

    /// Send a hello request to the network (does not await response)
    pub async fn hello_request(&self, peer_id: PeerId, request: HelloRequest) {
        trace!("Sending Hello Message {:?}", request);
        // TODO update to await response when we want to handle the latency
        self.network_send
            .send(NetworkMessage::HelloRequest { peer_id, request })
            .await;
    }
}
