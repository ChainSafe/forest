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

use crate::libp2p::{Gossipsub, build_gossipsub, build_subscription_filter, pubsub_topics};

const NETWORK: &str = "testnetname";

/// Swarm using Forest's subscription filter (the code under test).
fn filtered_swarm() -> Swarm<Gossipsub> {
    Swarm::new_ephemeral_tokio(|identity| {
        build_gossipsub(&identity, &NETWORK.into()).expect("failed to build gossipsub")
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
    let allowed: Vec<IdentTopic> = pubsub_topics(NETWORK).collect();
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
    let mut filter = build_subscription_filter(&NETWORK.into());
    for topic in pubsub_topics(NETWORK) {
        assert!(filter.can_subscribe(&topic.hash()));
    }
    assert!(!filter.can_subscribe(&IdentTopic::new("/cth/ulhu").hash()));
    assert!(!filter.can_subscribe(&TopicHash::from_raw("x".repeat(1 << 20))));
    // Wrong network suffix must not match.
    assert!(!filter.can_subscribe(&IdentTopic::new("/fil/blocks/lovecraftnet").hash()));
}

#[test]
fn filter_caps_are_set() {
    let filter = build_subscription_filter(&NETWORK.into());
    assert_eq!(filter.max_subscribed_topics, pubsub_topics(NETWORK).count());
    assert_eq!(filter.max_subscriptions_per_request, 100);
}
