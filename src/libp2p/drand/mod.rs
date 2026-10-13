// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod metrics;
#[cfg(test)]
pub(in crate::libp2p) mod test_utils;
mod validator;

pub(in crate::libp2p) use validator::DrandGossipValidator;

use std::{sync::Arc, time::Duration};

use libp2p::{
    PeerId,
    gossipsub::{IdentTopic, MessageId, TopicHash, TopicScoreParams, score_parameter_decay},
};

use super::{gossip_params::ExtraTopic, service::Verdict};
use crate::beacon::{Beacon as _, BeaconSchedule};

/// `gossipsub` topic prefix of a `drand` network, followed by its chain hash.
pub(in crate::libp2p) const PUBSUB_DRAND_STR: &str = "/drand/pubsub/v0.0.0";

/// The unchained beacon's `drand` topic and its validator.
pub struct DrandGossip {
    pub(in crate::libp2p) topic: TopicHash,
    pub(in crate::libp2p) verdicts: flume::Receiver<Verdict>,
    validator: DrandGossipValidator,
}

impl DrandGossip {
    /// `None` without an unchained beacon. `schedule` must be the one block validation uses, as
    /// gossip fills its cache.
    pub fn new(schedule: Arc<BeaconSchedule>) -> Option<Self> {
        let beacon = schedule.unchained_beacon()?;
        let topic = TopicHash::from_raw(format!("{PUBSUB_DRAND_STR}/{}", beacon.chain_hash()?));
        let (verdicts_tx, verdicts) = flume::unbounded();
        Some(Self {
            topic,
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
        self.validator
            .handle(data, message_id, source, unix_now_secs());
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

fn unix_now_secs() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}
