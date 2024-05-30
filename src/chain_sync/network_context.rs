// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    convert::TryFrom,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, SystemTime},
};

use crate::blocks::{FullTipset, Tipset, TipsetKey};
use crate::libp2p::{
    chain_exchange::{
        ChainExchangeRequest, ChainExchangeResponse, CompactedMessages, TipsetBundle, HEADERS,
        MESSAGES,
    },
    hello::{HelloRequest, HelloResponse},
    rpc::RequestResponseError,
    NetworkMessage, PeerId, PeerManager, BITSWAP_TIMEOUT,
};
use anyhow::Context as _;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use serde::de::DeserializeOwned;
use std::future::Future;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::{debug, trace, warn};

/// Timeout for response from an RPC request
// This value could be tweaked, this is just set pretty low to avoid peers
// timing out requests from slowing the node down. If increase, should create a
// countermeasure for this.
const CHAIN_EXCHANGE_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum number of concurrent chain exchange request being sent to the
/// network.
const MAX_CONCURRENT_CHAIN_EXCHANGE_REQUESTS: usize = 2;

/// Context used in chain sync to handle network requests.
/// This contains the peer manager, P2P service interface, and [`Blockstore`]
/// required to make network requests.
pub(in crate::chain_sync) struct SyncNetworkContext<DB> {
    /// Channel to send network messages through P2P service
    network_send: flume::Sender<NetworkMessage>,

    /// Manages peers to send requests to and updates request stats for the
    /// respective peers.
    peer_manager: Arc<PeerManager>,
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

/// Race tasks to completion while limiting the number of tasks that may execute concurrently.
/// Once a task finishes without error, the rest of the tasks are canceled.
struct RaceBatch<T> {
    tasks: JoinSet<Result<T, String>>,
    semaphore: Arc<Semaphore>,
}

impl<T> RaceBatch<T>
where
    T: Send + 'static,
{
    pub fn new(max_concurrent_jobs: usize) -> Self {
        RaceBatch {
            tasks: JoinSet::new(),
            semaphore: Arc::new(Semaphore::new(max_concurrent_jobs)),
        }
    }

    pub fn add(&mut self, future: impl Future<Output = Result<T, String>> + Send + 'static) {
        let sem = self.semaphore.clone();
        self.tasks.spawn(async move {
            let permit = sem
                .acquire_owned()
                .await
                .map_err(|_| "Semaphore unexpectedly closed")?;
            let result = future.await;
            drop(permit);
            result
        });
    }

    /// Return first finishing `Ok` future that passes validation else return `None` if all jobs failed
    pub async fn get_ok_validated<F>(mut self, validate: F) -> Option<T>
    where
        F: Fn(&T) -> bool,
    {
        while let Some(result) = self.tasks.join_next().await {
            if let Ok(Ok(value)) = result {
                if validate(&value) {
                    return Some(value);
                }
            }
        }
        // So far every task have failed
        None
    }
}

impl<DB> SyncNetworkContext<DB>
where
    DB: Blockstore,
{
    pub fn new(
        network_send: flume::Sender<NetworkMessage>,
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

    /// Send a `chain_exchange` request for only block headers (ignore
    /// messages). If `peer_id` is `None`, requests will be sent to a set of
    /// shuffled peers.
    pub async fn chain_exchange_headers(
        &self,
        peer_id: Option<PeerId>,
        tsk: &TipsetKey,
        count: u64,
    ) -> Result<Vec<Arc<Tipset>>, String> {
        self.handle_chain_exchange_request(peer_id, tsk, count, HEADERS, |_| true)
            .await
    }
    /// Send a `chain_exchange` request for only messages (ignore block
    /// headers). If `peer_id` is `None`, requests will be sent to a set of
    /// shuffled peers.
    pub async fn chain_exchange_messages(
        &self,
        peer_id: Option<PeerId>,
        tipsets: &[Arc<Tipset>],
    ) -> Result<Vec<CompactedMessages>, String> {
        let head = tipsets
            .last()
            .ok_or_else(|| "tipsets cannot be empty".to_owned())?;
        let tsk = head.key();
        tracing::debug!(
            "ChainExchange message sync tipsets: epoch: {}, len: {}",
            head.epoch(),
            tipsets.len()
        );
        self.handle_chain_exchange_request(
            peer_id,
            tsk,
            tipsets.len() as _,
            MESSAGES,
            |compacted_messages_vec: &Vec<CompactedMessages>| {
                for (msg, ts ) in compacted_messages_vec.iter().zip(tipsets.iter().rev()) {
                    let header_len = ts.block_headers().len();
                    if header_len != msg.bls_msg_includes.len()
                        || header_len != msg.secp_msg_includes.len()
                    {
                        tracing::warn!(
                            "header_len: {header_len}, msg.bls_msg_includes.len(): {}, msg.secp_msg_includes.len(): {}",
                            msg.bls_msg_includes.len(),
                            msg.secp_msg_includes.len()
                        );
                        return false;
                    }
                }
                true
            },
        )
        .await
    }

    /// Send a `chain_exchange` request for a single full tipset (includes
    /// messages) If `peer_id` is `None`, requests will be sent to a set of
    /// shuffled peers.
    pub async fn chain_exchange_fts(
        &self,
        peer_id: Option<PeerId>,
        tsk: &TipsetKey,
    ) -> Result<FullTipset, String> {
        let mut fts = self
            .handle_chain_exchange_request(peer_id, tsk, 1, HEADERS | MESSAGES, |_| true)
            .await?;

        if fts.len() != 1 {
            return Err(format!(
                "Full tipset request returned {} tipsets",
                fts.len()
            ));
        }
        Ok(fts.remove(0))
    }

    /// Requests that some content with a particular `Cid` get fetched over
    /// `Bitswap` if it doesn't exist in the `BlockStore`.
    pub async fn bitswap_get<TMessage: DeserializeOwned>(
        &self,
        content: Cid,
        epoch: Option<i64>,
    ) -> Result<TMessage, String> {
        // Check if what we are fetching over Bitswap already exists in the
        // database. If it does, return it, else fetch over the network.
        if let Some(b) = self.db.get_cbor(&content).map_err(|e| e.to_string())? {
            return Ok(b);
        }

        let (tx, rx) = flume::bounded(1);

        self.network_send
            .send_async(NetworkMessage::BitswapRequest {
                cid: content,
                response_channel: tx,
                epoch,
            })
            .await
            .map_err(|_| "failed to send bitswap request, network receiver dropped")?;

        let success = tokio::task::spawn_blocking(move || {
            rx.recv_timeout(BITSWAP_TIMEOUT).unwrap_or_default()
        })
        .await
        .is_ok();

        match self.db.get_cbor(&content) {
            Ok(Some(b)) => Ok(b),
            Ok(None) => Err(format!(
                "Not found in db, bitswap. success: {success} cid, {content:?}"
            )),
            Err(e) => Err(format!(
                "Error retrieving from db. success: {success} cid, {content:?}, {e}"
            )),
        }
    }

    /// Helper function to handle the peer retrieval if no peer supplied as well
    /// as the logging and updating of the peer info in the `PeerManager`.
    async fn handle_chain_exchange_request<T, F>(
        &self,
        peer_id: Option<PeerId>,
        tsk: &TipsetKey,
        request_len: u64,
        options: u64,
        validate: F,
    ) -> Result<Vec<T>, String>
    where
        T: TryFrom<TipsetBundle, Error = String> + Send + Sync + 'static,
        F: Fn(&Vec<T>) -> bool,
    {
        if request_len == 0 {
            return Ok(vec![]);
        }

        let request = ChainExchangeRequest {
            start: tsk.to_cids(),
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
                // No specific peer set, send requests to a shuffled set of top peers until
                // a request succeeds.
                let peers = self.peer_manager.top_peers_shuffled();

                let mut batch = RaceBatch::new(MAX_CONCURRENT_CHAIN_EXCHANGE_REQUESTS);
                for peer_id in peers.into_iter() {
                    let peer_manager = self.peer_manager.clone();
                    let network_send = self.network_send.clone();
                    let request = request.clone();
                    let network_failures = network_failures.clone();
                    let lookup_failures = lookup_failures.clone();
                    batch.add(async move {
                        match Self::chain_exchange_request(
                            peer_manager,
                            network_send,
                            peer_id,
                            request,
                        )
                        .await
                        {
                            Ok(chain_exchange_result) => {
                                match chain_exchange_result.into_result::<T>() {
                                    Ok(r) => Ok(r),
                                    Err(e) => {
                                        lookup_failures.fetch_add(1, Ordering::Relaxed);
                                        debug!("Failed chain_exchange response: {e}");
                                        Err(e)
                                    }
                                }
                            }
                            Err(e) => {
                                network_failures.fetch_add(1, Ordering::Relaxed);
                                debug!("Failed chain_exchange request to peer {peer_id:?}: {e}");
                                Err(e)
                            }
                        }
                    });
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

                let v = batch
                    .get_ok_validated(validate)
                    .await
                    .ok_or_else(make_failure_message)?;
                debug!("Succeed: handle_chain_exchange_request");
                v
            }
        };

        // Log success for the global request with the latency from before sending.
        match SystemTime::now().duration_since(global_pre_time) {
            Ok(t) => self.peer_manager.log_global_success(t),
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
        debug!("Sending ChainExchange Request to {peer_id}");

        let req_pre_time = SystemTime::now();

        let (tx, rx) = flume::bounded(1);
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
        // There is also a timeout inside the request-response calls, but this ensures
        // this.
        let res =
            tokio::task::spawn_blocking(move || rx.recv_timeout(CHAIN_EXCHANGE_TIMEOUT)).await;
        let res_duration = SystemTime::now()
            .duration_since(req_pre_time)
            .unwrap_or_default();
        match res {
            Ok(Ok(Ok(bs_res))) => {
                // Successful response
                peer_manager.log_success(peer_id, res_duration);
                debug!("Succeeded: ChainExchange Request to {peer_id}");
                Ok(bs_res)
            }
            Ok(Ok(Err(e))) => {
                // Internal libp2p error, score failure for peer and potentially disconnect
                match e {
                    RequestResponseError::UnsupportedProtocols => {
                        peer_manager
                            .ban_peer_with_default_duration(
                                peer_id,
                                "ChainExchange protocol unsupported",
                            )
                            .await;
                    }
                    RequestResponseError::ConnectionClosed | RequestResponseError::DialFailure => {
                        peer_manager.mark_peer_bad(peer_id, format!("chain exchange error {e:?}"));
                    }
                    // Ignore dropping peer on timeout for now. Can't be confident yet that the
                    // specified timeout is adequate time.
                    RequestResponseError::Timeout | RequestResponseError::Io(_) => {
                        peer_manager.log_failure(peer_id, res_duration);
                    }
                }
                debug!("Failed: ChainExchange Request to {peer_id}");
                Err(format!("Internal libp2p error: {e:?}"))
            }
            Ok(Err(_)) | Err(_) => {
                // Sender channel internally dropped or timeout, both should log failure which
                // will negatively score the peer, but not drop yet.
                peer_manager.log_failure(peer_id, res_duration);
                debug!("Timeout: ChainExchange Request to {peer_id}");
                Err(format!("Chain exchange request to {peer_id} timed out"))
            }
        }
    }

    /// Send a hello request to the network (does not immediately await
    /// response).
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

        const HELLO_TIMEOUT: Duration = Duration::from_secs(30);
        let sent = SystemTime::now();
        let res = tokio::task::spawn_blocking(move || rx.recv_timeout(HELLO_TIMEOUT))
            .await?
            .ok();
        Ok((peer_id, sent, res))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicBool, AtomicUsize};

    impl<T> RaceBatch<T>
    where
        T: Send + 'static,
    {
        pub async fn get_ok(self) -> Option<T> {
            self.get_ok_validated(|_| true).await
        }
    }

    #[tokio::test]
    async fn race_batch_ok() {
        let mut batch = RaceBatch::new(3);
        batch.add(async move { Ok(1) });
        batch.add(async move { Err("kaboom".into()) });

        assert_eq!(batch.get_ok().await, Some(1));
    }

    #[tokio::test]
    async fn race_batch_ok_faster() {
        let mut batch = RaceBatch::new(3);
        batch.add(async move {
            tokio::time::sleep(Duration::from_secs(100)).await;
            Ok(1)
        });
        batch.add(async move { Ok(2) });
        batch.add(async move { Err("kaboom".into()) });

        assert_eq!(batch.get_ok().await, Some(2));
    }

    #[tokio::test]
    async fn race_batch_none() {
        let mut batch: RaceBatch<i32> = RaceBatch::new(3);
        batch.add(async move { Err("kaboom".into()) });
        batch.add(async move { Err("banana".into()) });

        assert_eq!(batch.get_ok().await, None);
    }

    #[tokio::test]
    async fn race_batch_semaphore() {
        const MAX_JOBS: usize = 30;
        let counter = Arc::new(AtomicUsize::new(0));
        let exceeded = Arc::new(AtomicBool::new(false));

        let mut batch: RaceBatch<i32> = RaceBatch::new(MAX_JOBS);
        for _ in 0..10000 {
            let c = counter.clone();
            let e = exceeded.clone();
            batch.add(async move {
                let prev = c.fetch_add(1, Ordering::Relaxed);
                if prev >= MAX_JOBS {
                    e.fetch_or(true, Ordering::Relaxed);
                }

                tokio::task::yield_now().await;
                c.fetch_sub(1, Ordering::Relaxed);

                Err("banana".into())
            });
        }

        assert_eq!(batch.get_ok().await, None);
        assert!(!exceeded.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn race_batch_semaphore_exceeded() {
        const MAX_JOBS: usize = 30;
        let counter = Arc::new(AtomicUsize::new(0));
        let exceeded = Arc::new(AtomicBool::new(false));

        // We add one more job to exceed the limit
        let mut batch: RaceBatch<i32> = RaceBatch::new(MAX_JOBS + 1);
        for _ in 0..10000 {
            let c = counter.clone();
            let e = exceeded.clone();
            batch.add(async move {
                let prev = c.fetch_add(1, Ordering::Relaxed);
                if prev >= MAX_JOBS {
                    e.fetch_or(true, Ordering::Relaxed);
                }

                tokio::task::yield_now().await;
                c.fetch_sub(1, Ordering::Relaxed);

                Err("banana".into())
            });
        }

        assert_eq!(batch.get_ok().await, None);
        assert!(exceeded.load(Ordering::Relaxed));
    }
}
