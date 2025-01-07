// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    convert::TryFrom,
    num::NonZeroU64,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, SystemTime},
};

use crate::{
    blocks::{FullTipset, Tipset, TipsetKey},
    libp2p::{
        chain_exchange::{
            ChainExchangeRequest, ChainExchangeResponse, CompactedMessages, TipsetBundle, HEADERS,
            MESSAGES,
        },
        hello::{HelloRequest, HelloResponse},
        rpc::RequestResponseError,
        NetworkMessage, PeerId, PeerManager,
    },
    utils::{
        misc::{AdaptiveValueProvider, ExponentialAdaptiveValueProvider},
        stats::Stats,
    },
};
use anyhow::Context as _;
use fvm_ipld_blockstore::Blockstore;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::future::Future;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::{debug, trace, warn};

/// Timeout milliseconds for response from an RPC request
// This value is automatically adapted in the range of [5, 60] for different network conditions,
// being decreased on success and increased on failure
static CHAIN_EXCHANGE_TIMEOUT_MILLIS: Lazy<ExponentialAdaptiveValueProvider<u64>> =
    Lazy::new(|| ExponentialAdaptiveValueProvider::new(5000, 2000, 60000, false));

/// Maximum number of concurrent chain exchange request being sent to the
/// network.
const MAX_CONCURRENT_CHAIN_EXCHANGE_REQUESTS: usize = 2;

/// Context used in chain sync to handle network requests.
/// This contains the peer manager, P2P service interface, and [`Blockstore`]
/// required to make network requests.
pub struct SyncNetworkContext<DB> {
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

    /// Returns a reference to the channel for sending network messages through P2P service.
    pub fn network_send(&self) -> &flume::Sender<NetworkMessage> {
        &self.network_send
    }

    /// Send a `chain_exchange` request for only block headers (ignore
    /// messages). If `peer_id` is `None`, requests will be sent to a set of
    /// shuffled peers.
    pub async fn chain_exchange_headers(
        &self,
        peer_id: Option<PeerId>,
        tsk: &TipsetKey,
        count: NonZeroU64,
    ) -> Result<Vec<Arc<Tipset>>, String> {
        self.handle_chain_exchange_request(
            peer_id,
            tsk,
            count,
            HEADERS,
            |tipsets: &Vec<Arc<Tipset>>| validate_network_tipsets(tipsets, tsk),
        )
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
        tracing::trace!(
            "ChainExchange message sync tipsets: epoch: {}, len: {}",
            head.epoch(),
            tipsets.len()
        );
        self.handle_chain_exchange_request(
            peer_id,
            tsk,
            NonZeroU64::new(tipsets.len() as _).expect("Infallible"),
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
            .handle_chain_exchange_request(
                peer_id,
                tsk,
                NonZeroU64::new(1).expect("Infallible"),
                HEADERS | MESSAGES,
                |_| true,
            )
            .await?;

        if fts.len() != 1 {
            return Err(format!(
                "Full tipset request returned {} tipsets",
                fts.len()
            ));
        }
        Ok(fts.remove(0))
    }

    /// Helper function to handle the peer retrieval if no peer supplied as well
    /// as the logging and updating of the peer info in the `PeerManager`.
    async fn handle_chain_exchange_request<T, F>(
        &self,
        peer_id: Option<PeerId>,
        tsk: &TipsetKey,
        request_len: NonZeroU64,
        options: u64,
        validate: F,
    ) -> Result<Vec<T>, String>
    where
        T: TryFrom<TipsetBundle, Error = String> + Send + Sync + 'static,
        F: Fn(&Vec<T>) -> bool,
    {
        let request = ChainExchangeRequest {
            start: tsk.to_cids(),
            request_len: request_len.get(),
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
                if peers.is_empty() {
                    return Err("chain exchange failed: no peers are available".into());
                }
                let n_peers = peers.len();
                let mut batch = RaceBatch::new(MAX_CONCURRENT_CHAIN_EXCHANGE_REQUESTS);
                let success_time_cost_millis_stats = Arc::new(Mutex::new(Stats::new()));
                for peer_id in peers.into_iter() {
                    let peer_manager = self.peer_manager.clone();
                    let network_send = self.network_send.clone();
                    let request = request.clone();
                    let network_failures = network_failures.clone();
                    let lookup_failures = lookup_failures.clone();
                    let success_time_cost_millis_stats = success_time_cost_millis_stats.clone();
                    batch.add(async move {
                        let start = chrono::Utc::now();
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
                                    Ok(r) => {
                                        success_time_cost_millis_stats.lock().update(
                                            (chrono::Utc::now() - start).num_milliseconds(),
                                        );
                                        Ok(r)
                                    }
                                    Err(error) => {
                                        lookup_failures.fetch_add(1, Ordering::Relaxed);
                                        debug!(%peer_id, %request_len, %options, %n_peers, %error, "Failed chain_exchange response");
                                        Err(error)
                                    }
                                }
                            }
                            Err(error) => {
                                network_failures.fetch_add(1, Ordering::Relaxed);
                                debug!(%peer_id, %request_len, %options, %n_peers, %error, "Failed chain_exchange request to peer");
                                Err(error)
                            }
                        }
                    });
                }

                let make_failure_message = || {
                    CHAIN_EXCHANGE_TIMEOUT_MILLIS.adapt_on_failure();
                    tracing::info!(
                        "Increased chain exchange timeout to {}ms",
                        CHAIN_EXCHANGE_TIMEOUT_MILLIS.get()
                    );
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
                if let Ok(mean) = success_time_cost_millis_stats.lock().mean() {
                    if CHAIN_EXCHANGE_TIMEOUT_MILLIS.adapt_on_success(mean as _) {
                        tracing::info!(
                            "Decreased chain exchange timeout to {}ms. Current average: {}ms",
                            CHAIN_EXCHANGE_TIMEOUT_MILLIS.get(),
                            mean,
                        );
                    }
                }
                trace!("Succeed: handle_chain_exchange_request");
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
        trace!("Sending ChainExchange Request to {peer_id}");

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
        let res = tokio::task::spawn_blocking(move || {
            rx.recv_timeout(Duration::from_millis(CHAIN_EXCHANGE_TIMEOUT_MILLIS.get()))
        })
        .await;
        let res_duration = SystemTime::now()
            .duration_since(req_pre_time)
            .unwrap_or_default();
        match res {
            Ok(Ok(Ok(bs_res))) => {
                // Successful response
                peer_manager.log_success(&peer_id, res_duration);
                trace!("Succeeded: ChainExchange Request to {peer_id}");
                Ok(bs_res)
            }
            Ok(Ok(Err(e))) => {
                // Internal libp2p error, score failure for peer and potentially disconnect
                match e {
                    RequestResponseError::UnsupportedProtocols => {
                        // refactor this into Networkevent if user agent logging is critical here
                        peer_manager
                            .ban_peer_with_default_duration(
                                peer_id,
                                "ChainExchange protocol unsupported",
                                |_| None,
                            )
                            .await;
                    }
                    RequestResponseError::ConnectionClosed | RequestResponseError::DialFailure => {
                        peer_manager.mark_peer_bad(peer_id, format!("chain exchange error {e:?}"));
                    }
                    // Ignore dropping peer on timeout for now. Can't be confident yet that the
                    // specified timeout is adequate time.
                    RequestResponseError::Timeout | RequestResponseError::Io(_) => {
                        peer_manager.log_failure(&peer_id, res_duration);
                    }
                }
                debug!("Failed: ChainExchange Request to {peer_id}");
                Err(format!("Internal libp2p error: {e:?}"))
            }
            Ok(Err(_)) | Err(_) => {
                // Sender channel internally dropped or timeout, both should log failure which
                // will negatively score the peer, but not drop yet.
                peer_manager.log_failure(&peer_id, res_duration);
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

/// Validates network tipsets that are sorted by epoch in descending order with the below checks
/// 1. The latest(first) tipset has the desired tipset key
/// 2. The sorted tipsets are chained by their tipset keys
fn validate_network_tipsets(tipsets: &[Arc<Tipset>], start_tipset_key: &TipsetKey) -> bool {
    if let Some(start) = tipsets.first() {
        if start.key() != start_tipset_key {
            tracing::warn!(epoch=%start.epoch(), expected=%start_tipset_key, actual=%start.key(), "start tipset key mismatch");
            return false;
        }
        for (ts, pts) in tipsets.iter().zip(tipsets.iter().skip(1)) {
            if ts.parents() != pts.key() {
                tracing::warn!(epoch=%ts.epoch(), expected_parent=%pts.key(), actual_parent=%ts.parents(), "invalid chain");
                return false;
            }
        }
        true
    } else {
        tracing::warn!("invalid empty chain_exchange_headers response");
        false
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

    #[test]
    #[allow(unused_variables)]
    fn validate_network_tipsets_tests() {
        use crate::blocks::{chain4u, Chain4U};

        let c4u = Chain4U::new();
        chain4u! {
            in c4u;
            t0 @ [genesis_header]
            -> t1 @ [first_header]
            -> t2 @ [second_left, second_right]
            -> t3 @ [third]
            -> t4 @ [fourth]
        };
        let t0 = Arc::new(t0.clone());
        let t1 = Arc::new(t1.clone());
        let t2 = Arc::new(t2.clone());
        let t3 = Arc::new(t3.clone());
        let t4 = Arc::new(t4.clone());
        assert!(validate_network_tipsets(
            &[t4.clone(), t3.clone(), t2.clone(), t1.clone(), t0.clone()],
            t4.key()
        ));
        assert!(!validate_network_tipsets(
            &[t4.clone(), t3.clone(), t2.clone(), t1.clone(), t0.clone()],
            t3.key()
        ));
        assert!(!validate_network_tipsets(
            &[t4.clone(), t2.clone(), t1.clone(), t0.clone()],
            t4.key()
        ));
    }
}
