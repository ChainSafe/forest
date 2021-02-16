// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::sync::RwLock;
use blocks::Tipset;
use libp2p::core::PeerId;
use log::{debug, trace};
use rand::seq::SliceRandom;
use smallvec::SmallVec;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::{cmp::Ordering, collections::HashSet};

/// New peer multiplier slightly less than 1 to incentivize choosing new peers.
const NEW_PEER_MUL: f64 = 0.9;

/// Defines max number of peers to send each chain exchange request to.
pub(crate) const SHUFFLE_PEERS_PREFIX: usize = 16;

/// Local duration multiplier, affects duration delta change.
const LOCAL_INV_ALPHA: u32 = 5;
/// Global duration multiplier, affects duration delta change.
const GLOBAL_INV_ALPHA: u32 = 20;

#[derive(Debug, Default)]
/// Contains info about the peer's head [Tipset], as well as the request stats.
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
    fn new(head: Arc<Tipset>) -> Self {
        Self {
            head: Some(head),
            successes: 0,
            failures: 0,
            average_time: Default::default(),
        }
    }
}

/// Peer tracking sets, these are handled together to avoid race conditions or deadlocks
/// when updating state.
#[derive(Default)]
struct PeerSets {
    /// Map of full peers available.
    full_peers: HashMap<PeerId, PeerInfo>,

    /// Set of peers to ignore for being incompatible/ failing to accept connections.
    bad_peers: HashSet<PeerId>,
}

/// Thread safe peer manager which handles peer management for the `ChainExchange` protocol.
#[derive(Default)]
pub(crate) struct PeerManager {
    /// Full and bad peer sets.
    peers: RwLock<PeerSets>,

    /// Average response time from peers.
    avg_global_time: RwLock<Duration>,
}

impl PeerManager {
    /// Updates peer's heaviest tipset. If the peer does not exist in the set, a new `PeerInfo`
    /// will be generated.
    pub async fn update_peer_head(&self, peer_id: PeerId, ts: Arc<Tipset>) {
        let mut peers = self.peers.write().await;
        trace!("Updating head for PeerId {}", &peer_id);
        if let Some(pi) = peers.full_peers.get_mut(&peer_id) {
            pi.head = Some(ts);
        } else {
            peers.full_peers.insert(peer_id, PeerInfo::new(ts));
        }
    }

    /// Returns true if peer is not marked as bad or not already in set.
    pub async fn is_peer_new(&self, peer_id: &PeerId) -> bool {
        let peers = self.peers.read().await;
        !peers.bad_peers.contains(peer_id) && !peers.full_peers.contains_key(peer_id)
    }

    /// Sort peers based on a score function with the success rate and latency of requests.
    pub(crate) async fn sorted_peers(&self) -> Vec<PeerId> {
        let peer_lk = self.peers.read().await;
        let average_time = self.avg_global_time.read().await;
        let mut peers: Vec<_> = peer_lk
            .full_peers
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
        self.peers
            .read()
            .await
            .full_peers
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
    pub async fn log_success(&self, peer: PeerId, dur: Duration) {
        debug!("logging success for {:?}", peer);
        let mut peers = self.peers.write().await;
        peers.bad_peers.remove(&peer);
        let peer_stats = peers.full_peers.entry(peer).or_default();
        peer_stats.successes += 1;
        log_time(peer_stats, dur);
    }

    /// Logs a failure for the given peer, and updates the average request duration.
    pub async fn log_failure(&self, peer: PeerId, dur: Duration) {
        debug!("logging failure for {:?}", peer);
        let mut peers = self.peers.write().await;
        if !peers.bad_peers.contains(&peer) {
            let peer_stats = peers.full_peers.entry(peer).or_default();
            peer_stats.failures += 1;
            log_time(peer_stats, dur);
        }
    }

    /// Removes a peer from the set and returns true if the value was present previously
    pub async fn mark_peer_bad(&self, peer_id: PeerId) -> bool {
        let mut peers = self.peers.write().await;
        let removed = remove_peer(&mut peers, &peer_id);

        // Add peer to bad peer set if explicitly removed.
        debug!("marked peer {} bad", peer_id);
        peers.bad_peers.insert(peer_id);

        removed
    }

    /// Remove peer from managed set, does not mark as bad
    pub async fn remove_peer(&self, peer_id: &PeerId) -> bool {
        let mut peers = self.peers.write().await;
        debug!("removed peer {}", peer_id);
        remove_peer(&mut peers, peer_id)
    }

    /// Gets count of full peers managed. This is just used for testing.
    #[allow(dead_code)]
    pub async fn len(&self) -> usize {
        self.peers.read().await.full_peers.len()
    }
}

fn remove_peer(peers: &mut PeerSets, peer_id: &PeerId) -> bool {
    debug!(
        "removing peer {:?}, remaining chain exchange peers: {}",
        peer_id,
        peers.full_peers.len()
    );

    peers.full_peers.remove(peer_id).is_some()
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
