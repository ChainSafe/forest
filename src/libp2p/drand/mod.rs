// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod metrics;
#[cfg(test)]
pub(in crate::libp2p) mod test_utils;
mod validator;

pub(in crate::libp2p) use validator::DrandGossipValidator;

use std::{sync::Arc, time::Duration};

use ahash::HashMap;
use libp2p::{
    Multiaddr, PeerId,
    gossipsub::{IdentTopic, MessageId, TopicHash, TopicScoreParams, score_parameter_decay},
    multiaddr::Protocol,
};
use tracing::warn;

use super::{discovery::resolve_libp2p_dnsaddr, gossip_params::ExtraTopic, service::Verdict};
use crate::beacon::{Beacon as _, BeaconSchedule};

/// `gossipsub` topic prefix of a `drand` network, followed by its chain hash.
pub(in crate::libp2p) const PUBSUB_DRAND_STR: &str = "/drand/pubsub/v0.0.0";

/// The unchained beacon's `drand` topic, its relays and its validator.
pub struct DrandGossip {
    pub(in crate::libp2p) topic: TopicHash,
    pub(in crate::libp2p) relays: HashMap<PeerId, Multiaddr>,
    pub(in crate::libp2p) verdicts: flume::Receiver<Verdict>,
    validator: DrandGossipValidator,
}

impl DrandGossip {
    /// `None` without an unchained beacon. `schedule` must be the one block validation uses, as
    /// gossip fills its cache.
    pub async fn new(schedule: Arc<BeaconSchedule>) -> Option<Self> {
        let beacon = schedule.unchained_beacon()?;
        let topic = TopicHash::from_raw(format!("{PUBSUB_DRAND_STR}/{}", beacon.chain_hash()?));
        let relays = resolve_relays(beacon.relays()).await;
        let (verdicts_tx, verdicts) = flume::unbounded();
        Some(Self {
            topic,
            relays,
            verdicts,
            validator: DrandGossipValidator::new(schedule, verdicts_tx),
        })
    }

    /// The topic with its score parameters, for building `gossipsub`.
    pub(in crate::libp2p) fn extra_topic(&self) -> ExtraTopic {
        ExtraTopic {
            topic: IdentTopic::new(self.topic.as_str()),
            score: score_params(),
        }
    }

    /// Validates a topic message; the verdict arrives on [`Self::verdicts`].
    pub(in crate::libp2p) fn on_message(&self, message_id: MessageId, source: PeerId, data: &[u8]) {
        let now = chrono::Utc::now().timestamp().max(0) as u64;
        self.validator.handle(data, message_id, source, now);
    }
}

/// Same as Lotus: <https://github.com/filecoin-project/lotus/blob/d5e6eb3c28aa1630b3fe97b06dbf3cc9f30772a1/node/modules/lp2p/pubsub.go#L119>.
fn score_params() -> TopicScoreParams {
    TopicScoreParams {
        // 5x the block topic, as in Lotus
        topic_weight: 0.5,

        time_in_mesh_weight: 0.00027,
        time_in_mesh_quantum: Duration::from_secs(1),
        time_in_mesh_cap: 1.0,

        first_message_deliveries_weight: 5.0,
        first_message_deliveries_decay: score_parameter_decay(Duration::from_hours(1)),
        first_message_deliveries_cap: 25.0,

        // Disabled, as in Lotus: it pruned peers without their own beacon source.
        mesh_message_deliveries_weight: 0.0,
        mesh_message_deliveries_decay: 0.0,
        mesh_message_deliveries_cap: 0.0,
        mesh_message_deliveries_threshold: 0.0,
        mesh_message_deliveries_window: Duration::from_millis(0),
        mesh_message_deliveries_activation: Duration::from_millis(0),
        mesh_failure_penalty_weight: 0.0,
        mesh_failure_penalty_decay: 0.0,

        invalid_message_deliveries_weight: -1000.0,
        invalid_message_deliveries_decay: score_parameter_decay(Duration::from_hours(1)),
    }
}

/// Resolves the relays, skipping failures: HTTP is the fallback.
async fn resolve_relays(addrs: &[Multiaddr]) -> HashMap<PeerId, Multiaddr> {
    let mut relays = HashMap::default();
    for addr in addrs {
        if let Some(Protocol::Dnsaddr(name)) = addr.iter().next() {
            match resolve_libp2p_dnsaddr(&name).await {
                Ok(resolved) => relays.extend(
                    resolved
                        .into_iter()
                        .map(|(peer, addr)| (peer, addr.with(Protocol::P2p(peer)))),
                ),
                Err(e) => warn!("failed to resolve drand relay {addr}: {e:#}"),
            }
        } else if let Some(Protocol::P2p(peer)) = addr.iter().last() {
            relays.insert(peer, addr.clone());
        } else {
            warn!("ignoring drand relay {addr}: no peer id");
        }
    }
    relays
}
