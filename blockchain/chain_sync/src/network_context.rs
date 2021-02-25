// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::peer_manager::PeerManager;
use async_std::channel::Sender;
use async_std::future;
use blocks::{FullTipset, Tipset, TipsetKeys};
use cid::Cid;
use encoding::de::DeserializeOwned;
use forest_libp2p::{
    chain_exchange::{
        ChainExchangeRequest, ChainExchangeResponse, CompactedMessages, TipsetBundle, HEADERS,
        MESSAGES,
    },
    hello::{HelloRequest, HelloResponse},
    rpc::RequestResponseError,
    NetworkMessage,
};
use futures::{channel::oneshot::channel as oneshot_channel, Future};
use ipld_blockstore::BlockStore;
use libp2p::core::PeerId;
use log::{trace, warn};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use std::{convert::TryFrom, pin::Pin};

/// Future of the response from sending a hello request. This does not need to be immediately polled
/// because the response does not need to be handled synchronously.
pub(crate) type HelloResponseFuture = Pin<
    Box<
        dyn Future<
                Output = (
                    PeerId,
                    SystemTime,
                    Option<Result<HelloResponse, RequestResponseError>>,
                ),
            > + Send
            + Sync,
    >,
>;

/// Timeout for response from an RPC request
// TODO this value can be tweaked, this is just set pretty low to avoid peers timing out
// requests from slowing the node down. If increase, should create a countermeasure for this.
const RPC_TIMEOUT: u64 = 5;

/// Context used in chain sync to handle network requests.
/// This contains the peer manager, p2p service interface, and [BlockStore] required to make
/// network requests.
pub(crate) struct SyncNetworkContext<DB> {
    /// Channel to send network messages through p2p service
    network_send: Sender<NetworkMessage>,

    /// Manages peers to send requests to and updates request stats for the respective peers.
    pub peer_manager: Arc<PeerManager>,
    db: Arc<DB>,
}

impl<DB> Clone for SyncNetworkContext<DB> {
    fn clone(&self) -> Self {
        Self {
            network_send: self.network_send.clone(),
            peer_manager: self.peer_manager.clone(),
            db: self.db.clone(),
        }
    }
}

impl<DB> SyncNetworkContext<DB>
where
    DB: BlockStore + Sync + Send + 'static,
{
    pub fn new(
        network_send: Sender<NetworkMessage>,
        peer_manager: Arc<PeerManager>,
        db: Arc<DB>,
    ) -> Self {
        Self {
            network_send,
            peer_manager,
            db,
        }
    }

    /// Returns a reference to the peer manager of the network context.
    pub fn peer_manager(&self) -> &PeerManager {
        self.peer_manager.as_ref()
    }

    /// Send a chain_exchange request for only block headers (ignore messages).
    /// If `peer_id` is `None`, requests will be sent to a set of shuffled peers.
    pub async fn chain_exchange_headers(
        &self,
        peer_id: Option<PeerId>,
        tsk: &TipsetKeys,
        count: u64,
    ) -> Result<Vec<Arc<Tipset>>, String> {
        self.handle_chain_exchange_request(peer_id, tsk, count, HEADERS)
            .await
    }
    /// Send a chain_exchange request for only messages (ignore block headers).
    /// If `peer_id` is `None`, requests will be sent to a set of shuffled peers.
    pub async fn chain_exchange_messages(
        &self,
        peer_id: Option<PeerId>,
        tsk: &TipsetKeys,
        count: u64,
    ) -> Result<Vec<CompactedMessages>, String> {
        self.handle_chain_exchange_request(peer_id, tsk, count, MESSAGES)
            .await
    }

    /// Send a chain_exchange request for a single full tipset (includes messages)
    /// If `peer_id` is `None`, requests will be sent to a set of shuffled peers.
    pub async fn chain_exchange_fts(
        &self,
        peer_id: Option<PeerId>,
        tsk: &TipsetKeys,
    ) -> Result<FullTipset, String> {
        let mut fts = self
            .handle_chain_exchange_request(peer_id, tsk, 1, HEADERS | MESSAGES)
            .await?;

        if fts.len() != 1 {
            return Err(format!(
                "Full tipset request returned {} tipsets",
                fts.len()
            ));
        }
        Ok(fts.remove(0))
    }

    /// Requests that some content with a particular Cid get fetched over Bitswap if it doesn't
    /// exist in the BlockStore.
    pub async fn bitswap_get<TMessage: DeserializeOwned>(
        &self,
        content: Cid,
    ) -> Result<TMessage, String> {
        // Check if what we are fetching over Bitswap already exists in the
        // database. If it does, return it, else fetch over the network.
        if let Some(b) = self.db.get(&content).map_err(|e| e.to_string())? {
            return Ok(b);
        }
        let (tx, rx) = oneshot_channel();
        self.network_send
            .send(NetworkMessage::BitswapRequest {
                cid: content,
                response_channel: tx,
            })
            .await
            .map_err(|_| "failed to send bitswap request, network receiver dropped")?;
        let res = future::timeout(Duration::from_secs(RPC_TIMEOUT), rx).await;
        match res {
            Ok(Ok(())) => {
                match self.db.get(&content) {
                    Ok(Some(b)) => Ok(b),
                    Ok(None) => Err(format!("Bitswap response successful for: {:?}, but can't find it in the database", content)),
                    Err(e) => Err(format!("Bitswap response successful for: {:?}, but can't retreive it from the database: {}", content, e.to_string())),
                }
            }
            Err(_e) => {
               Err(format!("Bitswap get for {:?} timed out", content))
            }
            Ok(Err(e)) => {
                Err(format!("Bitswap get for {:?} failed: {}", content, e.to_string()))
            }
        }
    }

    /// Helper function to handle the peer retrieval if no peer supplied as well as the logging
    /// and updating of the peer info in the `PeerManager`.
    async fn handle_chain_exchange_request<T>(
        &self,
        peer_id: Option<PeerId>,
        tsk: &TipsetKeys,
        request_len: u64,
        options: u64,
    ) -> Result<Vec<T>, String>
    where
        T: TryFrom<TipsetBundle, Error = String>,
    {
        let request = ChainExchangeRequest {
            start: tsk.cids().to_vec(),
            request_len,
            options,
        };

        let global_pre_time = SystemTime::now();
        let bs_res = match peer_id {
            // Specific peer is given to send request, send specifically to that peer.
            Some(id) => self
                .chain_exchange_request(id, request)
                .await?
                .into_result()?,
            None => {
                // No specific peer set, send requests to a shuffled set of top peers until
                // a request succeeds.
                let peers = self.peer_manager.top_peers_shuffled().await;
                let mut res = None;
                for p in peers.into_iter() {
                    match self.chain_exchange_request(p, request.clone()).await {
                        Ok(bs_res) => match bs_res.into_result() {
                            Ok(r) => {
                                res = Some(r);
                                break;
                            }
                            Err(e) => {
                                warn!("Failed chain_exchange response: {}", e);
                                continue;
                            }
                        },
                        Err(e) => {
                            warn!("Failed chain_exchange request to peer {:?}: {}", p, e);
                            continue;
                        }
                    }
                }

                res.ok_or_else(|| "ChainExchange request failed for all top peers".to_string())?
            }
        };

        // Log success for the global request with the latency from before sending.
        match SystemTime::now().duration_since(global_pre_time) {
            Ok(t) => self.peer_manager.log_global_success(t).await,
            Err(e) => {
                warn!("logged time less than before request: {}", e);
            }
        }

        Ok(bs_res)
    }

    /// Send a chain_exchange request to the network and await response.
    async fn chain_exchange_request(
        &self,
        peer_id: PeerId,
        request: ChainExchangeRequest,
    ) -> Result<ChainExchangeResponse, String> {
        trace!("Sending ChainExchange Request {:?} to {}", request, peer_id);

        let req_pre_time = SystemTime::now();

        let (tx, rx) = oneshot_channel();
        if self
            .network_send
            .send(NetworkMessage::ChainExchangeRequest {
                peer_id,
                request,
                response_channel: tx,
            })
            .await
            .is_err()
        {
            return Err("Failed to send chain exchange request to network".to_string());
        };

        // Add timeout to receiving response from p2p service to avoid stalling.
        // There is also a timeout inside the request-response calls, but this ensures this.
        let res = future::timeout(Duration::from_secs(RPC_TIMEOUT), rx).await;
        let res_duration = SystemTime::now()
            .duration_since(req_pre_time)
            .unwrap_or_default();
        match res {
            Ok(Ok(Ok(bs_res))) => {
                // Successful response
                self.peer_manager.log_success(peer_id, res_duration).await;
                Ok(bs_res)
            }
            Ok(Ok(Err(e))) => {
                // Internal libp2p error, score failure for peer and potentially disconnect
                match e {
                    RequestResponseError::ConnectionClosed
                    | RequestResponseError::DialFailure
                    | RequestResponseError::UnsupportedProtocols => {
                        self.peer_manager.mark_peer_bad(peer_id).await;
                    }
                    // Ignore dropping peer on timeout for now. Can't be confident yet that the
                    // specified timeout is adequate time.
                    RequestResponseError::Timeout => {
                        self.peer_manager.log_failure(peer_id, res_duration).await;
                    }
                }
                Err(format!("Internal libp2p error: {:?}", e))
            }
            Ok(Err(_)) | Err(_) => {
                // Sender channel internally dropped or timeout, both should log failure which will
                // negatively score the peer, but not drop yet.
                self.peer_manager.log_failure(peer_id, res_duration).await;
                Err("Chain exchange request timed out".to_string())
            }
        }
    }

    /// Send a hello request to the network (does not immediately await response).
    pub async fn hello_request(
        &self,
        peer_id: PeerId,
        request: HelloRequest,
    ) -> Result<HelloResponseFuture, &'static str> {
        trace!("Sending Hello Message to {}", peer_id);

        // Create oneshot channel for receiving response from sent hello.
        let (tx, rx) = oneshot_channel();

        // Send request into libp2p service
        self.network_send
            .send(NetworkMessage::HelloRequest {
                peer_id,
                request,
                response_channel: tx,
            })
            .await
            .map_err(|_| "Failed to send hello request: receiver dropped")?;

        let sent = SystemTime::now();

        // Add timeout and create future to be polled asynchronously.
        let rx = future::timeout(Duration::from_secs(10), rx);
        Ok(Box::pin(async move {
            let res = rx.await;
            match res {
                // Convert timeout error into `Option` and wrap `Ok` with the PeerId and sent time.
                Ok(received) => (peer_id, sent, received.ok()),
                // Timeout on response, this doesn't matter to us, can safely ignore.
                Err(_) => (peer_id, sent, None),
            }
        }))
    }
}
