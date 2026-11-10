// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

use libp2p::gossipsub::MessageAcceptance;

use super::gossip_validation_test::gossip_message_event;
use crate::{
    beacon::gossipsub::{
        DrandGossip,
        test_utils::{EPOCH_ROUND, assert_verdict, fake_drand_schedule},
    },
    libp2p::service::handle_gossip_event,
};

/// `drand` rounds are decided by the listener, and nothing reaches the node.
#[tokio::test]
async fn drand_gossip_is_handed_to_the_listener() {
    let (drand, schedule) = fake_drand_schedule();
    let (gossip, listener) = DrandGossip::new(schedule).unwrap();
    tokio::spawn(listener.run());
    let (tx, rx) = flume::unbounded();

    let verdict = handle_gossip_event(
        gossip_message_event(drand.encode(EPOCH_ROUND), gossip.topic.clone()),
        &tx,
        "",
        "",
        Some(&gossip),
    )
    .await;

    assert!(verdict.is_none(), "the listener reports it");
    assert!(rx.try_recv().is_err(), "nothing is emitted to the node");
    let verdict = tokio::time::timeout(Duration::from_secs(5), gossip.verdicts.recv_async())
        .await
        .expect("no verdict reported")
        .unwrap();
    assert_verdict(&verdict.acceptance, MessageAcceptance::Accept);
}

/// A full listener queue drops the round instead of blocking the service.
#[tokio::test]
async fn full_listener_queue_ignores_the_round() {
    let (drand, schedule) = fake_drand_schedule();
    let (gossip, _listener) = DrandGossip::new(schedule).unwrap();
    let (tx, _rx) = flume::unbounded();
    let payload = drand.encode(EPOCH_ROUND);
    let send = || {
        handle_gossip_event(
            gossip_message_event(payload.clone(), gossip.topic.clone()),
            &tx,
            "",
            "",
            Some(&gossip),
        )
    };

    for _ in 0..gossip.rounds.capacity().unwrap() {
        assert!(send().await.is_none(), "queued for the listener");
    }
    let verdict = tokio::time::timeout(Duration::from_secs(5), send())
        .await
        .expect("the service must not wait for the listener")
        .expect("a verdict");
    assert_verdict(&verdict.acceptance, MessageAcceptance::Ignore);
}
