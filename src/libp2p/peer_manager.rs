// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    cmp::Ordering,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::blocks::Tipset;
use ahash::{HashMap, HashSet};
use flume::{Receiver, Sender};
use parking_lot::RwLock;
use rand::seq::SliceRandom;
use tracing::{debug, trace, warn};

use crate::libp2p::*;

/// New peer multiplier slightly less than 1 to incentivize choosing new peers.
const NEW_PEER_MUL: f64 = 0.9;

/// Defines max number of peers to send each chain exchange request to.
pub(in crate::libp2p) const SHUFFLE_PEERS_PREFIX: usize = 100;

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

/// Peer tracking sets, these are handled together to avoid race conditions or
/// deadlocks when updating state.
#[derive(Default)]
struct PeerSets {
    /// Map of full peers available.
    full_peers: HashMap<PeerId, PeerInfo>,
    /// Set of peers to ignore for being incompatible/ failing to accept
    /// connections.
    bad_peers: HashSet<PeerId>,
}

/// Thread safe peer manager which handles peer management for the
/// `ChainExchange` protocol.
pub struct PeerManager {
    /// Full and bad peer sets.
    peers: RwLock<PeerSets>,
    /// Average response time from peers.
    avg_global_time: RwLock<Duration>,
    /// Peer operation sender
    peer_ops_tx: Sender<PeerOperation>,
    /// Peer operation receiver
    peer_ops_rx: Receiver<PeerOperation>,
    /// Peer ban list, key is peer id, value is expiration time
    peer_ban_list: tokio::sync::RwLock<HashMap<PeerId, Option<Instant>>>,
}

impl Default for PeerManager {
    fn default() -> Self {
        let (peer_ops_tx, peer_ops_rx) = flume::unbounded();
        PeerManager {
            peers: Default::default(),
            avg_global_time: Default::default(),
            peer_ops_tx,
            peer_ops_rx,
            peer_ban_list: Default::default(),
        }
    }
}

impl PeerManager {
    /// Updates peer's heaviest tipset. If the peer does not exist in the set, a
    /// new `PeerInfo` will be generated.
    pub fn update_peer_head(&self, peer_id: PeerId, ts: Arc<Tipset>) {
        let mut peers = self.peers.write();
        trace!("Updating head for PeerId {}", &peer_id);
        if let Some(pi) = peers.full_peers.get_mut(&peer_id) {
            pi.head = Some(ts);
        } else {
            peers.full_peers.insert(peer_id, PeerInfo::new(ts));
            metrics::FULL_PEERS.inc();
        }
    }

    /// Gets the head epoch of a peer
    pub fn get_peer_head_epoch(&self, peer_id: &PeerId) -> Option<i64> {
        let peers = self.peers.read();
        peers
            .full_peers
            .get(peer_id)
            .and_then(|pi| pi.head.as_ref().map(|ts| ts.epoch()))
    }

    /// Returns true if peer is not marked as bad or not already in set.
    pub fn is_peer_new(&self, peer_id: &PeerId) -> bool {
        let peers = self.peers.read();
        !peers.bad_peers.contains(peer_id) && !peers.full_peers.contains_key(peer_id)
    }

    /// Sort peers based on a score function with the success rate and latency
    /// of requests.
    pub(in crate::libp2p) fn sorted_peers(&self) -> Vec<PeerId> {
        let peer_lk = self.peers.read();
        let average_time = self.avg_global_time.read();
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

    /// Return shuffled slice of ordered peers from the peer manager. Ordering
    /// is based on failure rate and latency of the peer.
    pub fn top_peers_shuffled(&self) -> Vec<PeerId> {
        let mut peers: Vec<_> = self
            .sorted_peers()
            .into_iter()
            .take(SHUFFLE_PEERS_PREFIX)
            .collect();

        // Shuffle top peers, to avoid sending all requests to same predictable peer.
        peers.shuffle(&mut rand::rngs::OsRng);

        peers
    }

    /// Logs a global request success. This just updates the average for the
    /// peer manager.
    pub fn log_global_success(&self, dur: Duration) {
        debug!("logging global success");
        let mut avg_global = self.avg_global_time.write();
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

    /// Logs a success for the given peer, and updates the average request
    /// duration.
    pub fn log_success(&self, peer: PeerId, dur: Duration) {
        debug!("logging success for {:?}", peer);
        let mut peers = self.peers.write();
        // Attempt to remove the peer and decrement bad peer count
        if peers.bad_peers.remove(&peer) {
            metrics::BAD_PEERS.dec();
        };
        // If the peer is not already accounted for, increment full peer count
        if !peers.full_peers.contains_key(&peer) {
            metrics::FULL_PEERS.inc();
        }
        let peer_stats = peers.full_peers.entry(peer).or_default();
        peer_stats.successes += 1;
        log_time(peer_stats, dur);
    }

    /// Logs a failure for the given peer, and updates the average request
    /// duration.
    pub fn log_failure(&self, peer: PeerId, dur: Duration) {
        debug!("logging failure for {:?}", peer);
        let mut peers = self.peers.write();
        if !peers.bad_peers.contains(&peer) {
            metrics::PEER_FAILURE_TOTAL.inc();
            if !peers.full_peers.contains_key(&peer) {
                metrics::FULL_PEERS.inc();
            }
            let peer_stats = peers.full_peers.entry(peer).or_default();
            peer_stats.failures += 1;
            log_time(peer_stats, dur);
        }
    }

    /// Removes a peer from the set and returns true if the value was present
    /// previously
    pub fn mark_peer_bad(&self, peer_id: PeerId, reason: impl Into<String>) -> bool {
        let mut peers = self.peers.write();
        let removed = remove_peer(&mut peers, &peer_id);
        if removed {
            metrics::FULL_PEERS.dec();
        }

        // Add peer to bad peer set
        let reason = reason.into();
        tracing::warn!(%peer_id, %reason, "marked peer bad");
        if peers.bad_peers.insert(peer_id) {
            metrics::BAD_PEERS.inc();
        }

        removed
    }

    /// Remove peer from managed set, does not mark as bad
    pub fn remove_peer(&self, peer_id: &PeerId) -> bool {
        let mut peers = self.peers.write();
        debug!("removed peer {}", peer_id);
        let removed = remove_peer(&mut peers, peer_id);
        if removed {
            metrics::FULL_PEERS.dec();
        }
        removed
    }

    /// Gets peer operation receiver
    pub fn peer_ops_rx(&self) -> &Receiver<PeerOperation> {
        &self.peer_ops_rx
    }

    /// Bans a peer with an optional duration
    pub async fn ban_peer(
        &self,
        peer: PeerId,
        reason: impl Into<String>,
        duration: Option<Duration>,
    ) {
        let mut locked = self.peer_ban_list.write().await;
        locked.insert(peer, duration.and_then(|d| Instant::now().checked_add(d)));
        if let Err(e) = self
            .peer_ops_tx
            .send_async(PeerOperation::Ban(peer, reason.into()))
            .await
        {
            warn!("ban_peer err: {e}");
        }
    }

    /// Bans a peer with the default duration(1h)
    pub async fn ban_peer_with_default_duration(&self, peer: PeerId, reason: impl Into<String>) {
        const BAN_PEER_DURATION: Duration = Duration::from_secs(60 * 60); //1h
        self.ban_peer(peer, reason, Some(BAN_PEER_DURATION)).await
    }

    pub async fn peer_operation_event_loop_task(self: Arc<Self>) -> anyhow::Result<()> {
        let mut unban_list = vec![];
        loop {
            unban_list.clear();

            let now = Instant::now();
            for (peer, expiration) in self.peer_ban_list.read().await.iter() {
                if let Some(expiration) = expiration {
                    if &now > expiration {
                        unban_list.push(*peer);
                    }
                }
            }
            if !unban_list.is_empty() {
                {
                    let mut locked = self.peer_ban_list.write().await;
                    for peer in unban_list.iter() {
                        locked.remove(peer);
                    }
                }
                for &peer in unban_list.iter() {
                    if let Err(e) = self
                        .peer_ops_tx
                        .send_async(PeerOperation::Unban(peer))
                        .await
                    {
                        warn!("unban_peer err: {e}");
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }

    pub fn peer_count(&self) -> usize {
        self.peers.read().full_peers.len()
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

pub enum PeerOperation {
    Ban(PeerId, String),
    Unban(PeerId),
}
