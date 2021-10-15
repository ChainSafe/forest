// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{PUBSUB_BLOCK_STR, PUBSUB_MSG_STR};
use libp2p::gossipsub::{
    score_parameter_decay, IdentTopic, PeerScoreParams, PeerScoreThresholds, TopicScoreParams,
};
use std::{collections::HashMap, time::Duration};

// All these parameters are copied from what Lotus has set for their Topic scores.
// They are currently unused because enabling them causes GossipSub blocks to come
// delayed usually by 1 second compared to when we have these parameters disabled.
// Leaving these here so that we can enable and fix these parameters when they are needed.

#[allow(dead_code)]
fn build_msg_topic_config() -> TopicScoreParams {
    TopicScoreParams {
        // expected 10 blocks/min
        topic_weight: 0.1,

        // 1 tick per second, maxes at 1 after 1 hour (1/3600)
        time_in_mesh_weight: 0.0002778,
        time_in_mesh_quantum: Duration::from_secs(1),
        time_in_mesh_cap: 1.0,

        // deliveries decay after 10min, cap at 100 tx
        first_message_deliveries_weight: 0.5,
        first_message_deliveries_decay: score_parameter_decay(Duration::from_secs(10 * 60)), // 10mins
        // 100 blocks in an hour
        first_message_deliveries_cap: 100.0,

        // Set to 0 because disabled for Filecoin
        mesh_message_deliveries_weight: 0.0,
        mesh_message_deliveries_decay: 0.0,
        mesh_message_deliveries_cap: 0.0,
        mesh_message_deliveries_threshold: 0.0,
        mesh_message_deliveries_window: Duration::from_millis(0),
        mesh_message_deliveries_activation: Duration::from_millis(0),
        mesh_failure_penalty_weight: 0.0,
        mesh_failure_penalty_decay: 0.0,

        // invalid messages decay after 1 hour
        invalid_message_deliveries_weight: -1000.0,
        invalid_message_deliveries_decay: score_parameter_decay(Duration::from_secs(60 * 60)),
    }
}

#[allow(dead_code)]
fn build_block_topic_config() -> TopicScoreParams {
    TopicScoreParams {
        topic_weight: 0.1,

        // 1 tick per second, maxes at 1 hours (-1/3600)
        time_in_mesh_weight: 0.00027,
        time_in_mesh_quantum: Duration::from_secs(1),
        time_in_mesh_cap: 1.0,

        // deliveries decay after 10min, cap at 100 blocks
        first_message_deliveries_weight: 5.0,
        first_message_deliveries_decay: score_parameter_decay(Duration::from_secs(60 * 60)),
        // 100 blocks in 10 minutes
        first_message_deliveries_cap: 100.0,

        // Set to 0 because disabled for Filecoin
        mesh_message_deliveries_weight: 0.0,
        mesh_message_deliveries_decay: 0.0,
        mesh_message_deliveries_cap: 0.0,
        mesh_message_deliveries_threshold: 0.0,
        mesh_message_deliveries_window: Duration::from_millis(0),
        mesh_message_deliveries_activation: Duration::from_millis(0),
        mesh_failure_penalty_weight: 0.0,
        mesh_failure_penalty_decay: 0.0,

        // invalid messages decay after 1 hour
        invalid_message_deliveries_weight: -1000.0,
        invalid_message_deliveries_decay: score_parameter_decay(Duration::from_secs(60 * 60)),
    }
}

#[allow(dead_code)]
pub(crate) fn build_peer_score_params(network_name: &str) -> PeerScoreParams {
    let mut psp_topics = HashMap::new();

    // msg topic
    let msg_topic = IdentTopic::new(format!("{}/{}", PUBSUB_MSG_STR, network_name));
    psp_topics.insert(msg_topic.hash(), build_msg_topic_config());
    // block topic
    let block_topic = IdentTopic::new(format!("{}/{}", PUBSUB_BLOCK_STR, network_name));
    psp_topics.insert(block_topic.hash(), build_block_topic_config());

    PeerScoreParams {
        app_specific_weight: 1.0,

        ip_colocation_factor_threshold: 5.0,
        ip_colocation_factor_weight: -100.0,
        ip_colocation_factor_whitelist: Default::default(),

        behaviour_penalty_threshold: 6.0,
        behaviour_penalty_weight: -10.0,
        behaviour_penalty_decay: score_parameter_decay(Duration::from_secs(60 * 60)),

        decay_interval: Duration::from_secs(1),
        decay_to_zero: 0.01,

        topic_score_cap: 0.0,

        retain_score: Duration::from_secs(6 * 60 * 60),
        topics: psp_topics,
    }
}

#[allow(dead_code)]
pub(crate) fn build_peer_score_threshold() -> PeerScoreThresholds {
    PeerScoreThresholds {
        gossip_threshold: -500.0,
        publish_threshold: -1000.0,
        graylist_threshold: -2500.0,
        accept_px_threshold: 1000.0,
        opportunistic_graft_threshold: 3.5,
    }
}
