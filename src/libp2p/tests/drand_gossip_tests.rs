// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

use libp2p::gossipsub::{MessageAcceptance, TopicHash};

use super::gossip_validation_test::gossip_message_event;
use crate::{
    beacon::gossipsub::{
        DrandGossip, PUBSUB_DRAND_STR,
        test_utils::{EPOCH_ROUND, assert_verdict, fake_drand_schedule},
    },
    libp2p::{NetworkMessage, service::handle_gossip_event},
};

fn drand_topic() -> TopicHash {
    TopicHash::from_raw(format!("{PUBSUB_DRAND_STR}/chain-hash"))
}

#[tokio::test]
async fn drand_round_is_handed_to_drand_gossip() {
    let (drand, schedule) = fake_drand_schedule();
    let (rounds_tx, rounds_rx) = flume::bounded(1);
    let (network_tx, network_rx) = flume::unbounded();
    tokio::spawn(DrandGossip::new(schedule, rounds_rx, network_tx).run());
    let (tx, rx) = flume::unbounded();

    let verdict = handle_gossip_event(
        gossip_message_event(drand.encode(EPOCH_ROUND), drand_topic()),
        &tx,
        "",
        "",
        &rounds_tx,
    )
    .await;

    assert!(verdict.is_none(), "reported later");
    assert!(rx.try_recv().is_err(), "nothing is emitted to the node");
    let message = tokio::time::timeout(Duration::from_secs(5), network_rx.recv_async())
        .await
        .expect("no verdict reported")
        .unwrap();
    let NetworkMessage::GossipVerdict(verdict) = message else {
        panic!("unexpected message: {message:?}");
    };
    assert_verdict(&verdict.acceptance, MessageAcceptance::Accept);
}

/// A full queue drops the round instead of blocking the service.
#[tokio::test]
async fn full_queue_ignores_the_round() {
    let (rounds_tx, _rounds_rx) = flume::bounded(1);
    let (tx, _rx) = flume::unbounded();
    let send = || {
        handle_gossip_event(
            gossip_message_event(vec![], drand_topic()),
            &tx,
            "",
            "",
            &rounds_tx,
        )
    };

    assert!(send().await.is_none(), "queued");
    let verdict = tokio::time::timeout(Duration::from_secs(5), send())
        .await
        .expect("the service must not wait for the queue")
        .expect("a verdict");
    assert_verdict(&verdict.acceptance, MessageAcceptance::Ignore);
}
