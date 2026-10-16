// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod metrics;
#[cfg(test)]
pub(in crate::libp2p) mod test_utils;
mod validator;

pub(in crate::libp2p) use validator::DrandGossipValidator;

use std::sync::Arc;

use libp2p::{
    PeerId,
    gossipsub::{MessageId, TopicHash},
};

use super::service::Verdict;
use crate::beacon::{Beacon as _, BeaconSchedule};

/// `gossipsub` topic prefix of a `drand` network, followed by its chain hash.
const PUBSUB_DRAND_STR: &str = "/drand/pubsub/v0.0.0";

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

    /// Validates a topic message; the verdict arrives on [`Self::verdicts`].
    pub(in crate::libp2p) fn on_message(&self, message_id: MessageId, source: PeerId, data: &[u8]) {
        self.validator
            .handle(data, message_id, source, unix_now_secs());
    }
}

fn unix_now_secs() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}
