// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

use futures::StreamExt as _;
use libp2p::{
    Swarm,
    gossipsub::{self, IdentTopic, MessageAcceptance},
    swarm::SwarmEvent,
};
use libp2p_swarm_test::SwarmExt as _;

use super::gossip_validation_test::gossip_message_event;
use crate::{
    beacon::{
        gossipsub::{
            DrandGossip, PUBSUB_DRAND_STR, Round,
            test_utils::{EPOCH_ROUND, assert_verdict, fake_drand_schedule, is_cached},
        },
        tests::fake_drand_signer::FakeDrandSigner,
    },
    libp2p::{
        Gossipsub, build_gossipsub,
        gossip_params::ExtraTopic,
        pubsub_topics,
        service::{Verdict, handle_gossip_event},
    },
};

const NETWORK: &str = "testdrandgossipsub";

fn forest_node(drand: &DrandGossip) -> Swarm<Gossipsub> {
    let extra = [ExtraTopic::drand(&drand.topic)];
    let mut node = Swarm::new_ephemeral_tokio(|id| {
        build_gossipsub(&id, &NETWORK.into(), &extra).expect("failed to build gossipsub")
    });
    for topic in pubsub_topics(NETWORK) {
        node.behaviour_mut().subscribe(&topic).unwrap();
    }
    node.behaviour_mut().subscribe(&extra[0].topic).unwrap();
    node
}

fn plain_peer(topic: &IdentTopic) -> Swarm<gossipsub::Behaviour> {
    let mut peer = Swarm::new_ephemeral_tokio(|id| {
        gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(id),
            gossipsub::ConfigBuilder::default().build().unwrap(),
        )
        .unwrap()
    });
    peer.behaviour_mut().subscribe(topic).unwrap();
    peer
}

fn drand_topic(drand: &FakeDrandSigner) -> IdentTopic {
    IdentTopic::new(format!("{PUBSUB_DRAND_STR}/{}", drand.chain_info_hash()))
}

/// Relay, Forest node, observer: the node forwards only verified rounds.
#[tokio::test]
async fn only_verified_rounds_are_forwarded() {
    let (drand, schedule) = fake_drand_schedule();
    let (gossip, listener) = DrandGossip::new(schedule.clone()).expect("unchained beacon");
    tokio::spawn(listener.run());
    let topic = drand_topic(&drand);
    assert_eq!(gossip.topic, topic.hash());

    let mut node = forest_node(&gossip);
    let mut relay = plain_peer(&topic);
    let mut observer = plain_peer(&topic);
    node.listen().with_memory_addr_external().await;
    relay.connect(&mut node).await;
    observer.connect(&mut node).await;

    // Wait for the mesh on both links.
    let hash = topic.hash();
    tokio::time::timeout(Duration::from_secs(10), async {
        while relay.behaviour().mesh_peers(&hash).next().is_none()
            || observer.behaviour().mesh_peers(&hash).next().is_none()
            || node.behaviour().mesh_peers(&hash).count() < 2
        {
            tokio::select! {
                _ = node.select_next_some() => {}
                _ = relay.select_next_some() => {}
                _ = observer.select_next_some() => {}
            }
        }
    })
    .await
    .expect("drand topic mesh never formed");

    // Forged first, so if forwarded it would arrive before the valid one.
    let forged = FakeDrandSigner::encode_raw(EPOCH_ROUND, drand.entry(EPOCH_ROUND + 1).signature());
    let valid = drand.encode(EPOCH_ROUND);
    relay
        .behaviour_mut()
        .publish(topic.clone(), forged)
        .unwrap();
    relay
        .behaviour_mut()
        .publish(topic.clone(), valid.clone())
        .unwrap();

    let mut verdicts = vec![];
    let mut delivered = vec![];
    tokio::time::timeout(Duration::from_secs(10), async {
        while verdicts.len() < 2 || delivered.is_empty() {
            tokio::select! {
                _ = relay.select_next_some() => {}
                ev = node.select_next_some() => {
                    if let SwarmEvent::Behaviour(gossipsub::Event::Message {
                        propagation_source: source, message_id, message,
                    }) = ev {
                        let round = Round { message_id, source, data: message.data };
                        gossip.rounds.try_send(round).unwrap();
                    }
                }
                ev = observer.select_next_some() => {
                    if let SwarmEvent::Behaviour(gossipsub::Event::Message { message, .. }) = ev {
                        delivered.push(message.data);
                    }
                }
                Ok(Verdict { message_id, source, acceptance }) = gossip.verdicts.recv_async() => {
                    verdicts.push(matches!(acceptance, MessageAcceptance::Reject));
                    node.behaviour_mut()
                        .report_message_validation_result(&message_id, &source, acceptance);
                }
            }
        }
    })
    .await
    .expect("rounds never verified and forwarded");

    assert_eq!(delivered, vec![valid], "only the valid round is forwarded");
    let rejected = verdicts.iter().filter(|rejected| **rejected).count();
    assert_eq!(rejected, 1, "the forged round, and only it, is rejected");

    assert!(is_cached(&schedule, EPOCH_ROUND));
}

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
