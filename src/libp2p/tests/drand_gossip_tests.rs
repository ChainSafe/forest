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
            test_utils::{EPOCH_ROUND, assert_verdict, fake_drand_schedule},
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

fn forest_node(extra_topic: ExtraTopic) -> Swarm<Gossipsub> {
    let mut node = Swarm::new_ephemeral_tokio(|id| {
        build_gossipsub(&id, &NETWORK.into(), std::slice::from_ref(&extra_topic))
            .expect("failed to build gossipsub")
    });
    for topic in pubsub_topics(NETWORK) {
        node.behaviour_mut().subscribe(&topic).unwrap();
    }
    node.behaviour_mut().subscribe(&extra_topic.topic).unwrap();
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

    let mut node = forest_node(ExtraTopic::drand(&gossip.topic));
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
                    // `MessageAcceptance::Reject`` has no Copy or Clone
                    // so it is not possible to push the verdicts
                    // and report_message_validation_result
                    match acceptance {
                        MessageAcceptance::Reject => verdicts.push(MessageAcceptance::Reject),
                        MessageAcceptance::Accept => verdicts.push(MessageAcceptance::Accept),
                        MessageAcceptance::Ignore => verdicts.push(MessageAcceptance::Ignore),
                    }

                    node.behaviour_mut()
                        .report_message_validation_result(&message_id, &source, acceptance);
                }
            }
        }
    })
    .await
    .expect("rounds never verified and forwarded");

    assert_eq!(delivered, vec![valid], "only the valid round is forwarded");

    let expected = [
        (MessageAcceptance::Reject, 1),
        (MessageAcceptance::Accept, 1),
        (MessageAcceptance::Ignore, 0),
    ];
    for (acc, exp_count) in &expected {
        // `MessageAcceptance` has no `PartialEq`
        let actual_count = verdicts
            .iter()
            .filter(|v| std::mem::discriminant(*v) == std::mem::discriminant(acc))
            .count();
        assert_eq!(actual_count, *exp_count, "{acc:?} verdicts");
    }

    assert!(schedule.unchained_beacon().unwrap().has_round(EPOCH_ROUND));
}
