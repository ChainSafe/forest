// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Trims excess peers instead of refusing new connections: a hard cap rejects peers before
//! identify, so a well-connected node would look unreachable to crawlers and new peers.

use std::{
    cmp::Reverse,
    time::{Duration, Instant},
};

use ahash::HashMap;
use itertools::Itertools as _;
use libp2p::PeerId;

use super::hello::INBOUND_HELLO_WAIT_TIMEOUT;

/// Peers are trimmed once connections exceed `target_peer_count` times this.
pub(in crate::libp2p) const HIGH_WATER_PER_TARGET_PEER: u32 = 4;
/// Peers are trimmed until connections are down to `target_peer_count` times this.
const LOW_WATER_PER_TARGET_PEER: u32 = 3;

/// How much a connected peer is worth keeping. Lower values are pruned first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::libp2p) enum PeerValue {
    /// Not (yet) a useful Filecoin peer, e.g. no completed hello.
    Low,
    /// A peer we sync from.
    High,
    /// Never pruned.
    Protected,
}

struct ConnectedPeer {
    since: Instant,
    outbound: bool,
    connections: usize,
}

pub(in crate::libp2p) struct ConnectionManager {
    low_water: usize,
    high_water: usize,
    peers: HashMap<PeerId, ConnectedPeer>,
}

impl ConnectionManager {
    /// Leaves new peers time to identify, be crawled, and send their hello.
    const GRACE_PERIOD: Duration = INBOUND_HELLO_WAIT_TIMEOUT;

    pub fn new(target_peer_count: u32) -> Self {
        Self {
            low_water: target_peer_count.saturating_mul(LOW_WATER_PER_TARGET_PEER) as usize,
            high_water: target_peer_count.saturating_mul(HIGH_WATER_PER_TARGET_PEER) as usize,
            peers: HashMap::default(),
        }
    }

    pub fn on_connection_established(&mut self, peer: PeerId, outbound: bool, now: Instant) {
        self.peers
            .entry(peer)
            .or_insert(ConnectedPeer {
                since: now,
                outbound,
                connections: 0,
            })
            .connections += 1;
    }

    pub fn on_connection_closed(&mut self, peer: &PeerId, remaining: usize) {
        if remaining == 0 {
            self.peers.remove(peer);
        } else if let Some(p) = self.peers.get_mut(peer) {
            p.connections = remaining;
        }
    }

    /// Peers to disconnect to get back to the low-water mark, least valuable first. Within the same
    /// value, inbound peers go before outbound ones, as peers we dialed are harder for an attacker
    /// to place, and newer peers before older ones.
    ///
    /// Only connections past the grace period and not protected count towards the low-water mark,
    /// so a flood of short-lived connections cannot evict established peers.
    pub fn peers_to_prune(
        &self,
        now: Instant,
        value: impl Fn(&PeerId) -> PeerValue,
    ) -> Vec<PeerId> {
        if self.peers.values().map(|p| p.connections).sum::<usize>() <= self.high_water {
            return vec![];
        }
        let candidates = self
            .peers
            .iter()
            .filter(|(_, p)| now.duration_since(p.since) >= Self::GRACE_PERIOD)
            .map(|(id, p)| (value(id), p.outbound, Reverse(p.since), *id, p.connections))
            .filter(|(value, ..)| *value != PeerValue::Protected)
            .sorted_unstable()
            .collect_vec();
        let mut excess = candidates
            .iter()
            .map(|(.., connections)| connections)
            .sum::<usize>()
            .saturating_sub(self.low_water);
        candidates
            .into_iter()
            .take_while(|(.., connections)| {
                let take = excess > 0;
                excess = excess.saturating_sub(*connections);
                take
            })
            .map(|(.., id, _)| id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRACE: Duration = ConnectionManager::GRACE_PERIOD;

    /// Low water 3, high water 4. Returns the manager and the time its peers have the given ages at.
    fn manager_with(peers: &[(PeerId, bool, Duration)]) -> (ConnectionManager, Instant) {
        let now = Instant::now() + GRACE * 10;
        let mut manager = ConnectionManager::new(1);
        for (peer, outbound, age) in peers {
            manager.on_connection_established(*peer, *outbound, now - *age);
        }
        (manager, now)
    }

    #[test]
    fn nothing_to_prune_at_or_below_high_water() {
        let peers = (0..5)
            .map(|_| (PeerId::random(), false, GRACE * 2))
            .collect_vec();
        let (mut manager, now) = manager_with(&peers);
        let (first, ..) = peers.first().unwrap();
        manager.on_connection_closed(first, 0);
        assert!(manager.peers_to_prune(now, |_| PeerValue::Low).is_empty());
    }

    #[test]
    fn prunes_down_to_low_water_by_value_direction_and_age() {
        let old_inbound_low = PeerId::random();
        let new_inbound_low = PeerId::random();
        let outbound_low = PeerId::random();
        let inbound_high = PeerId::random();
        let protected = PeerId::random();
        let (mut manager, now) = manager_with(&[
            (old_inbound_low, false, GRACE * 3),
            (new_inbound_low, false, GRACE * 2),
            (outbound_low, true, GRACE * 2),
            (inbound_high, false, GRACE * 2),
            (protected, false, GRACE * 2),
        ]);
        // A second connection counts towards the water marks.
        manager.on_connection_established(inbound_high, false, now);
        let value = |p: &PeerId| match p {
            p if *p == inbound_high => PeerValue::High,
            p if *p == protected => PeerValue::Protected,
            _ => PeerValue::Low,
        };
        assert_eq!(
            manager.peers_to_prune(now, value),
            vec![new_inbound_low, old_inbound_low]
        );
    }

    #[test]
    fn connections_in_grace_period_do_not_evict_established_peers() {
        let established = (0..3).map(|_| (PeerId::random(), false, GRACE * 2));
        let flood = (0..10).map(|_| (PeerId::random(), false, GRACE / 2));
        let (manager, now) = manager_with(&established.chain(flood).collect_vec());
        assert!(manager.peers_to_prune(now, |_| PeerValue::Low).is_empty());
    }
}
