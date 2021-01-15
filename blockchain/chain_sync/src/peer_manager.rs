// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::sync::RwLock;
use blocks::Tipset;
use libp2p::core::PeerId;
use log::{debug, trace, warn};
use rand::seq::SliceRandom;
use smallvec::SmallVec;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// New peer multiplier slightly less than 1 to incentivize choosing new peers.
const NEW_PEER_MUL: f64 = 0.9;

/// Defines max number of peers to send each chain exchange request to.
pub(crate) const SHUFFLE_PEERS_PREFIX: usize = 16;

/// Local duration multiplier, affects duration delta change.
const LOCAL_INV_ALPHA: u32 = 5;
/// Global duration multiplier, affects duration delta change.
const GLOBAL_INV_ALPHA: u32 = 20;

#[derive(Debug)]
struct PeerInfo {
    /// Head tipset received from hello message.
    head: Option<Arc<Tipset>>,
    /// Number of successful requests.
    successes: u32,
    /// Number of failed requests.
    failures: u32,
    /// Average response time for the peer.
    average_time: Duration,
}

impl PeerInfo {
    fn new(head: Option<Arc<Tipset>>) -> Self {
        Self {
            head,
            successes: 0,
            failures: 0,
            average_time: Default::default(),
        }
    }
}

/// Thread safe peer manager which handles peer management for the `ChainExchange` protocol.
#[derive(Default)]
pub struct PeerManager {
    /// Hash set of full peers available
    full_peers: RwLock<HashMap<PeerId, PeerInfo>>,

    /// Average response time from peers
    avg_global_time: RwLock<Duration>,
}

impl PeerManager {
    /// Updates peer's heaviest tipset. If the peer does not exist in the set, a new `PeerInfo`
    /// will be generated.
    pub async fn update_peer_head(&self, peer_id: PeerId, ts: Option<Arc<Tipset>>) {
        let mut fp = self.full_peers.write().await;
        trace!("Updating head for PeerId {}", &peer_id);
        if let Some(pi) = fp.get_mut(&peer_id) {
            pi.head = ts;
        } else {
            fp.insert(peer_id, PeerInfo::new(ts));
        }
    }

    /// Returns true if peer set is empty
    pub async fn is_empty(&self) -> bool {
        self.full_peers.read().await.is_empty()
    }

    /// Sort peers based on a score function with the success rate and latency of requests.
    pub(crate) async fn sorted_peers(&self) -> Vec<PeerId> {
        let peer_lk = self.full_peers.read().await;
        let average_time = self.avg_global_time.read().await;
        let mut peers: Vec<_> = peer_lk
            .iter()
            .map(|(p, info)| {
                let cost = if (info.successes + info.failures) > 0 {
                    // Calculate cost based on fail rate and latency
                    let fail_rate = f64::from(info.failures) / f64::from(info.successes);
                    info.average_time.as_secs_f64() + fail_rate * average_time.as_secs_f64()
                } else {
                    // There have been no failures or successes
                    average_time.as_secs_f64() * NEW_PEER_MUL
                };
                (p, cost)
            })
            .collect();

        // Unstable sort because hashmap iter order doesn't need to be preserved.
        peers.sort_unstable_by(|(_, v1), (_, v2)| v1.partial_cmp(v2).unwrap_or(Ordering::Equal));
        peers.into_iter().map(|(p, _)| p).cloned().collect()
    }

    /// Return shuffled slice of ordered peers from the peer manager. Ordering is based
    /// on failure rate and latency of the peer.
    pub async fn top_peers_shuffled(&self) -> SmallVec<[PeerId; SHUFFLE_PEERS_PREFIX]> {
        let mut peers: SmallVec<_> = self
            .sorted_peers()
            .await
            .into_iter()
            .take(SHUFFLE_PEERS_PREFIX)
            .collect();

        // Shuffle top peers, to avoid sending all requests to same predictable peer.
        peers.shuffle(&mut rand::thread_rng());

        peers
    }

    /// Retrieves all head tipsets from current peer set.
    pub async fn get_peer_heads(&self) -> Vec<Arc<Tipset>> {
        self.full_peers
            .read()
            .await
            .iter()
            .filter_map(|(_, v)| v.head.clone())
            .collect()
    }

    /// Logs a global request success. This just updates the average for the peer manager.
    pub async fn log_global_success(&self, dur: Duration) {
        debug!("logging global success");
        let mut avg_global = self.avg_global_time.write().await;
        if *avg_global == Duration::default() {
            *avg_global = dur;
        } else if dur < *avg_global {
            let delta = (*avg_global - dur) / GLOBAL_INV_ALPHA;
            *avg_global -= delta
        } else {
            let delta = (dur - *avg_global) / GLOBAL_INV_ALPHA;
            *avg_global += delta
        }
    }

    /// Logs a success for the given peer, and updates the average request duration.
    pub async fn log_success(&self, peer: &PeerId, dur: Duration) {
        debug!("logging success for {:?}", peer);
        match self.full_peers.write().await.get_mut(peer) {
            Some(p) => {
                p.successes += 1;
                log_time(p, dur);
            }
            None => warn!("log success called for peer not in peer manager ({})", peer),
        }
    }

    /// Logs a failure for the given peer, and updates the average request duration.
    pub async fn log_failure(&self, peer: &PeerId, dur: Duration) {
        debug!("logging failure for {:?}", peer);
        match self.full_peers.write().await.get_mut(peer) {
            Some(p) => {
                p.failures += 1;
                log_time(p, dur);
            }
            None => warn!("log success called for peer not in peer manager ({})", peer),
        }
    }

    /// Removes a peer from the set and returns true if the value was present previously
    pub async fn remove_peer(&self, peer_id: &PeerId) -> bool {
        let mut peers = self.full_peers.write().await;
        debug!(
            "removing peer {:?}, remaining chain exchange peers: {}",
            peer_id,
            peers.len()
        );
        peers.remove(peer_id).is_some()
    }

    /// Gets count of full peers managed.
    pub async fn len(&self) -> usize {
        self.full_peers.read().await.len()
    }
}

fn log_time(info: &mut PeerInfo, dur: Duration) {
    if info.average_time == Duration::default() {
        info.average_time = dur;
    } else if dur < info.average_time {
        let delta = (info.average_time - dur) / LOCAL_INV_ALPHA;
        info.average_time -= delta
    } else {
        let delta = (dur - info.average_time) / LOCAL_INV_ALPHA;
        info.average_time += delta
    }
}
