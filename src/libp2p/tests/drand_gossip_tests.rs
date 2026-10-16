// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

use super::gossip_validation_test::gossip_message_event;
use crate::libp2p::{
    drand::{
        DrandGossip,
        test_utils::{EPOCH_ROUND, fake_drand_schedule},
    },
    service::handle_gossip_event,
};

/// `drand` verdicts are deferred to the `drand` module, and nothing reaches the node.
#[tokio::test]
async fn drand_gossip_is_handed_to_the_drand_module() {
    let (drand, schedule) = fake_drand_schedule();
    let gossip = DrandGossip::new(schedule).unwrap();
    let (tx, rx) = flume::unbounded();

    let verdict = handle_gossip_event(
        gossip_message_event(drand.encode(EPOCH_ROUND), gossip.topic.clone()),
        &tx,
        "",
        "",
        Some(&gossip),
    )
    .await;

    assert!(verdict.is_none(), "the drand verdict is deferred");
    assert!(rx.try_recv().is_err(), "nothing is emitted to the node");
    tokio::time::timeout(Duration::from_secs(5), gossip.verdicts.recv_async())
        .await
        .expect("no verdict reported")
        .unwrap();
}
