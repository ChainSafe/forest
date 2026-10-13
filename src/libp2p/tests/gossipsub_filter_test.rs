// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Tests for the gossipsub subscription filter, which bounds the topics a peer
//! can make the node track to Forest's whitelist.

use std::time::Duration;

use futures::StreamExt as _;
use libp2p::{
    Swarm,
    gossipsub::{self, IdentTopic, MessageAuthenticity, TopicHash, TopicSubscriptionFilter},
    swarm::SwarmEvent,
};
use libp2p_swarm_test::SwarmExt as _;

use crate::libp2p::{
    Gossipsub, build_gossipsub, build_subscription_filter, drand::PUBSUB_DRAND_STR,
    gossip_params::ExtraTopic, pubsub_topics,
};

const NETWORK: &str = "testnetname";

fn drand_topic() -> Vec<ExtraTopic> {
    vec![ExtraTopic {
        topic: IdentTopic::new(format!(
            "{PUBSUB_DRAND_STR}/52db9ba70e0cc0f6eaf7803dd07447a1f5477735fd3f661792ba94600c84e971"
        )),
        score: Default::default(),
    }]
}

fn allowed_topics() -> Vec<IdentTopic> {
    pubsub_topics(NETWORK)
        .chain(drand_topic().into_iter().map(|extra| extra.topic))
        .collect()
}

/// Swarm using Forest's subscription filter (the code under test).
fn filtered_swarm() -> Swarm<Gossipsub> {
    Swarm::new_ephemeral_tokio(|identity| {
        build_gossipsub(&identity, &NETWORK.into(), &drand_topic())
            .expect("failed to build gossipsub")
    })
}

/// Swarm with the default (unrestricted) subscription filter.
fn unfiltered_swarm() -> Swarm<gossipsub::Behaviour> {
    Swarm::new_ephemeral_tokio(|identity| {
        let config = gossipsub::ConfigBuilder::default()
            .build()
            .expect("valid config");
        gossipsub::Behaviour::new(MessageAuthenticity::Signed(identity), config)
            .expect("failed to build gossipsub")
    })
}

/// Only whitelisted topics are tracked, regardless of how many others a peer
/// announces.
#[tokio::test]
async fn only_whitelisted_topics_are_tracked() {
    let mut node = filtered_swarm();
    let mut peer = unfiltered_swarm();

    node.listen().with_memory_addr_external().await;
    peer.connect(&mut node).await;

    // Non-whitelisted topics first, then the whitelisted ones. Ordering is
    // preserved, so seeing the whitelisted subscriptions means the earlier ones
    // were already processed.
    for i in 0..1_000 {
        let unlisted = IdentTopic::new(format!("/other/topic/{i}"));
        peer.behaviour_mut().subscribe(&unlisted).unwrap();
    }
    let allowed = allowed_topics();
    for topic in &allowed {
        peer.behaviour_mut().subscribe(topic).unwrap();
    }

    tokio::spawn(peer.loop_on_next());

    let allowed_hashes: Vec<_> = allowed.iter().map(|t| t.hash()).collect();
    let mut observed = 0;
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let SwarmEvent::Behaviour(gossipsub::Event::Subscribed { topic, .. }) =
                node.select_next_some().await
            {
                assert!(
                    allowed_hashes.contains(&topic),
                    "node tracked a non-whitelisted topic: {topic}"
                );
                observed += 1;
                if observed == allowed_hashes.len() {
                    break;
                }
            }
        }
    })
    .await
    .expect("timed out waiting for whitelisted subscriptions");
}

#[test]
fn filter_allows_only_whitelisted_topics() {
    let mut filter = build_subscription_filter(&NETWORK.into(), &drand_topic());
    for topic in allowed_topics() {
        assert!(filter.can_subscribe(&topic.hash()));
    }
    assert!(!filter.can_subscribe(&IdentTopic::new("/cth/ulhu").hash()));
    assert!(!filter.can_subscribe(&TopicHash::from_raw("x".repeat(1 << 20))));
    // Wrong network suffix must not match.
    assert!(!filter.can_subscribe(&IdentTopic::new("/fil/blocks/lovecraftnet").hash()));
    assert!(!filter.can_subscribe(&IdentTopic::new("/drand/pubsub/v0.0.0/deadbeef").hash()));
}

#[test]
fn filter_caps_are_set() {
    let filter = build_subscription_filter(&NETWORK.into(), &drand_topic());
    assert_eq!(filter.max_subscribed_topics, allowed_topics().len());
    assert_eq!(filter.max_subscriptions_per_request, 100);
}
