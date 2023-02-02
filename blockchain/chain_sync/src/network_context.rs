// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::Context;
use cid::Cid;
use forest_blocks::{FullTipset, Tipset, TipsetKeys};
use forest_encoding::de::DeserializeOwned;
use forest_libp2p::{
    chain_exchange::{
        ChainExchangeRequest, ChainExchangeResponse, CompactedMessages, TipsetBundle, HEADERS,
        MESSAGES,
    },
    hello::{HelloRequest, HelloResponse},
    rpc::RequestResponseError,
    NetworkMessage, PeerId, PeerManager, BITSWAP_TIMEOUT,
};
use forest_utils::db::BlockstoreExt;
use futures::channel::oneshot::channel as oneshot_channel;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::clock::ChainEpoch;
use log::{debug, trace, warn};
use std::sync::{atomic::Ordering, Arc};
use std::time::{Duration, SystemTime};
use std::{convert::TryFrom, sync::atomic::AtomicU64};
use tokio::{task::JoinSet, time::timeout};

/// Timeout for response from an RPC request
// TODO this value can be tweaked, this is just set pretty low to avoid peers timing out
// requests from slowing the node down. If increase, should create a countermeasure for this.
const CHAIN_EXCHANGE_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum number of concurrent chain exchange request being sent to the network
const MAX_CONCURRENT_CHAIN_EXCHANGE_REQUESTS: usize = 2;

/// Context used in chain sync to handle network requests.
/// This contains the peer manager, P2P service interface, and [`BlockStore`] required to make
/// network requests.
pub(crate) struct SyncNetworkContext<DB> {
    /// Channel to send network messages through P2P service
    network_send: flume::Sender<NetworkMessage>,

    /// Manages peers to send requests to and updates request stats for the respective peers.
    pub peer_manager: Arc<PeerManager>,
    db: Box<DB>,
}

impl<DB: Clone> Clone for SyncNetworkContext<DB> {
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
    DB: Blockstore,
{
    pub fn new(
        network_send: flume::Sender<NetworkMessage>,
        peer_manager: Arc<PeerManager>,
        db: DB,
    ) -> Self {
        Self {
            network_send,
            peer_manager,
            db: Box::new(db),
        }
    }

    /// Returns a reference to the peer manager of the network context.
    pub fn peer_manager(&self) -> &PeerManager {
        self.peer_manager.as_ref()
    }

    /// Send a `chain_exchange` request for only block headers (ignore messages).
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
    /// Send a `chain_exchange` request for only messages (ignore block headers).
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

    /// Send a `chain_exchange` request for a single full tipset (includes messages)
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

    /// Requests that some content with a particular `Cid` get fetched over `Bitswap` if it doesn't
    /// exist in the `BlockStore`.
    pub async fn bitswap_get<TMessage: DeserializeOwned>(
        &self,
        epoch: ChainEpoch,
        content: Cid,
    ) -> Result<TMessage, String> {
        // Check if what we are fetching over Bitswap already exists in the
        // database. If it does, return it, else fetch over the network.
        if let Some(b) = self.db.get_obj(&content).map_err(|e| e.to_string())? {
            return Ok(b);
        }

        let (tx, rx) = flume::bounded(1);

        self.network_send
            .send_async(NetworkMessage::BitswapRequest {
                epoch,
                cid: content,
                response_channel: tx,
            })
            .await
            .map_err(|_| "failed to send bitswap request, network receiver dropped")?;

        let success = tokio::task::spawn_blocking(move || {
            rx.recv_timeout(BITSWAP_TIMEOUT).unwrap_or_default()
        })
        .await
        .is_ok();

        match self.db.get_obj(&content) {
            Ok(Some(b)) => Ok(b),
            Ok(None) => Err(format!(
                "Not found in db, bitswap. success: {success} cid, {content:?}"
            )),
            Err(e) => Err(format!(
                "Error retrieving from db. success: {success} cid, {content:?}, {e}"
            )),
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
        T: TryFrom<TipsetBundle, Error = String> + Send + Sync + 'static,
    {
        let request = ChainExchangeRequest {
            start: tsk.cids().to_vec(),
            request_len,
            options,
        };

        let global_pre_time = SystemTime::now();
        let network_failures = Arc::new(AtomicU64::new(0));
        let lookup_failures = Arc::new(AtomicU64::new(0));
        let chain_exchange_result = match peer_id {
            // Specific peer is given to send request, send specifically to that peer.
            Some(id) => Self::chain_exchange_request(
                self.peer_manager.clone(),
                self.network_send.clone(),
                id,
                request,
            )
            .await?
            .into_result()?,
            None => {
                // Control max num of concurrent jobs
                let (n_task_control_tx, n_task_control_rx) =
                    flume::bounded(MAX_CONCURRENT_CHAIN_EXCHANGE_REQUESTS);
                let (result_tx, result_rx) = flume::bounded::<Vec<T>>(1);
                // No specific peer set, send requests to a shuffled set of top peers until
                // a request succeeds.
                let peers = self.peer_manager.top_peers_shuffled().await;
                let mut tasks = JoinSet::new();
                for peer_id in peers.into_iter() {
                    let n_task_control_tx = n_task_control_tx.clone();
                    let n_task_control_rx = n_task_control_rx.clone();
                    let result_tx = result_tx.clone();
                    let peer_manager = self.peer_manager.clone();
                    let network_send = self.network_send.clone();
                    let request = request.clone();
                    let network_failures = network_failures.clone();
                    let lookup_failures = lookup_failures.clone();
                    tasks.spawn(async move {
                        if n_task_control_tx.send_async(()).await.is_ok() {
                            match Self::chain_exchange_request(
                                peer_manager,
                                network_send,
                                peer_id,
                                request,
                            )
                            .await
                            {
                                Ok(chain_exchange_result) => {
                                    match chain_exchange_result.into_result() {
                                        Ok(r) => {
                                            _ = result_tx.send_async(r).await;
                                        }
                                        Err(e) => {
                                            lookup_failures.fetch_add(1, Ordering::Relaxed);
                                            _ = n_task_control_rx.recv_async().await;
                                            debug!("Failed chain_exchange response: {e}");
                                        }
                                    }
                                }
                                Err(e) => {
                                    network_failures.fetch_add(1, Ordering::Relaxed);
                                    _ = n_task_control_rx.recv_async().await;
                                    debug!(
                                        "Failed chain_exchange request to peer {peer_id:?}: {e}"
                                    );
                                }
                            }
                        }
                    });
                }

                async fn wait_all<T: 'static>(tasks: &mut JoinSet<T>) {
                    while tasks.join_next().await.is_some() {}
                }

                let make_failure_message = || {
                    let mut message = String::new();
                    message.push_str("ChainExchange request failed for all top peers. ");
                    message.push_str(&format!(
                        "{} network failures, ",
                        network_failures.load(Ordering::Relaxed)
                    ));
                    message.push_str(&format!(
                        "{} lookup failures, ",
                        lookup_failures.load(Ordering::Relaxed)
                    ));
                    message.push_str(&format!("request:\n{request:?}",));
                    message
                };

                tokio::select! {
                    result = result_rx.recv_async() => {
                        tasks.abort_all();
                        log::debug!("Succeed: handle_chain_exchange_request");
                        result.map_err(|e| e.to_string())?
                    },
                    _ = wait_all(&mut tasks) => return Err(make_failure_message()),
                }
            }
        };

        // Log success for the global request with the latency from before sending.
        match SystemTime::now().duration_since(global_pre_time) {
            Ok(t) => self.peer_manager.log_global_success(t).await,
            Err(e) => {
                warn!("logged time less than before request: {}", e);
            }
        }

        Ok(chain_exchange_result)
    }

    /// Send a `chain_exchange` request to the network and await response.
    async fn chain_exchange_request(
        peer_manager: Arc<PeerManager>,
        network_send: flume::Sender<NetworkMessage>,
        peer_id: PeerId,
        request: ChainExchangeRequest,
    ) -> Result<ChainExchangeResponse, String> {
        log::debug!("Sending ChainExchange Request to {peer_id}");

        let req_pre_time = SystemTime::now();

        let (tx, rx) = oneshot_channel();
        if network_send
            .send_async(NetworkMessage::ChainExchangeRequest {
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
        let res = timeout(CHAIN_EXCHANGE_TIMEOUT, rx).await;
        let res_duration = SystemTime::now()
            .duration_since(req_pre_time)
            .unwrap_or_default();
        match res {
            Ok(Ok(Ok(bs_res))) => {
                // Successful response
                peer_manager.log_success(peer_id, res_duration).await;
                log::debug!("Succeeded: ChainExchange Request to {peer_id}");
                Ok(bs_res)
            }
            Ok(Ok(Err(e))) => {
                // Internal libp2p error, score failure for peer and potentially disconnect
                match e {
                    RequestResponseError::ConnectionClosed
                    | RequestResponseError::DialFailure
                    | RequestResponseError::UnsupportedProtocols => {
                        peer_manager.mark_peer_bad(peer_id).await;
                    }
                    // Ignore dropping peer on timeout for now. Can't be confident yet that the
                    // specified timeout is adequate time.
                    RequestResponseError::Timeout => {
                        peer_manager.log_failure(peer_id, res_duration).await;
                    }
                }
                log::debug!("Failed: ChainExchange Request to {peer_id}");
                Err(format!("Internal libp2p error: {e:?}"))
            }
            Ok(Err(_)) | Err(_) => {
                // Sender channel internally dropped or timeout, both should log failure which will
                // negatively score the peer, but not drop yet.
                peer_manager.log_failure(peer_id, res_duration).await;
                log::debug!("Timeout: ChainExchange Request to {peer_id}");
                Err(format!("Chain exchange request to {peer_id} timed out"))
            }
        }
    }

    /// Send a hello request to the network (does not immediately await response).
    pub async fn hello_request(
        &self,
        peer_id: PeerId,
        request: HelloRequest,
    ) -> anyhow::Result<(PeerId, SystemTime, Option<HelloResponse>)> {
        trace!("Sending Hello Message to {}", peer_id);

        // Create oneshot channel for receiving response from sent hello.
        let (tx, rx) = flume::bounded(1);

        // Send request into libp2p service
        self.network_send
            .send_async(NetworkMessage::HelloRequest {
                peer_id,
                request,
                response_channel: tx,
            })
            .await
            .context("Failed to send hello request: receiver dropped")?;

        const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
        let sent = SystemTime::now();
        let res = tokio::task::spawn_blocking(move || rx.recv_timeout(HELLO_TIMEOUT))
            .await?
            .ok();
        Ok((peer_id, sent, res))
    }
}
