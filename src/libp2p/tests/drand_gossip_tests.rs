// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{sync::Arc, time::Duration};

use futures::StreamExt as _;
use libp2p::{
    Swarm,
    gossipsub::{self, IdentTopic},
    swarm::SwarmEvent,
};
use libp2p_swarm_test::SwarmExt as _;

use crate::chain_sync::handle_drand_entry;
use crate::libp2p::{
    NetworkEvent, NetworkMessage, PUBSUB_DRAND_STR, PubsubMessage, PubsubTopic, build_gossipsub,
    service::handle_gossip_event,
};
use crate::networks::GenesisNetworkName;
use crate::{
    beacon::{
        Beacon, BeaconPoint, BeaconSchedule,
        tests::fake_drand::{
            FAKE_DRAND_GENESIS_TIME, FAKE_DRAND_PERIOD, FakeDrand, TEST_FIL_BLOCK_DELAY,
            TEST_FIL_GENESIS_TIME,
        },
    },
    libp2p::{Gossipsub, PubsubTopicCfg},
};
use libp2p::gossipsub::MessageAcceptance;
use tokio::sync::Semaphore;

/// End to end: a relay publishes rounds, the node's `gossipsub` delivers them,
/// `handle_gossip_event` decodes them and defers the verdict, and `handle_drand_entry`
/// verifies, caches and accepts them.
#[tokio::test]
async fn gossip_rounds_are_verified_and_cached() {
    let drand = FakeDrand::new(vec![], FAKE_DRAND_PERIOD, FAKE_DRAND_GENESIS_TIME);
    let hash = drand.chain_info_hash();
    let schedule = Arc::new(BeaconSchedule(vec![BeaconPoint::new(
        0,
        drand.beacon(TEST_FIL_GENESIS_TIME, TEST_FIL_BLOCK_DELAY),
    )]));
    let beacon = schedule.unchained_beacon().expect("unchained beacon");

    let topic = IdentTopic::new(format!("{PUBSUB_DRAND_STR}/{hash}"));
    let mut kinds = ahash::HashMap::default();
    kinds.insert(topic.hash(), PubsubTopic::Drand);

    // `PubsubTopicCfg` borrows, so these have to outlive the swarm construction.
    // The whitelist must carry the *fake* chain hash, otherwise the node refuses
    // to subscribe to the topic the relay publishes on.
    let network_name: GenesisNetworkName = "testdrandgossipsub".into();
    let drand_chain_hashes = vec![hash];
    let cfg = PubsubTopicCfg {
        network_name: &network_name,
        drand_chain_hashes: &drand_chain_hashes,
    };

    let mut node = Swarm::new_ephemeral_tokio(|id| build_gossipsub(&id, cfg).unwrap());

    let mut relay = Swarm::new_ephemeral_tokio(|id| {
        gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(id),
            gossipsub::ConfigBuilder::default().build().unwrap(),
        )
        .unwrap()
    });

    node.listen().with_memory_addr_external().await;
    relay.connect(&mut node).await;

    relay.behaviour_mut().subscribe(&topic).unwrap();
    node.behaviour_mut().subscribe(&topic).unwrap();

    wait_until_meshed(&mut node, &mut relay, &topic).await;

    let (events_tx, events_rx) = flume::unbounded();
    let (network_send, verdicts) = flume::unbounded();
    let limiter = Arc::new(Semaphore::new(1));

    for round in 1..=5u64 {
        relay
            .behaviour_mut()
            .publish(topic.clone(), drand.to_protobuf(round))
            .unwrap();
        let event = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                tokio::select! {
                    _ = relay.select_next_some() => {},
                    ev = node.select_next_some() => {
                        if let SwarmEvent::Behaviour(ev @ gossipsub::Event::Message { .. }) = ev {
                            break ev;
                        }
                    }
                }
            }
        })
        .await
        .expect("no gossip message");

        assert!(
            handle_gossip_event(event, &events_tx, &kinds)
                .await
                .is_none(),
            "drand verdicts are deferred to the chain follower"
        );
        let Ok(NetworkEvent::PubsubMessage {
            message:
                PubsubMessage::DrandEntry {
                    entry,
                    message_id,
                    source,
                },
        }) = events_rx.try_recv()
        else {
            panic!("no drand entry emitted for round {round}");
        };
        assert_eq!(entry.round(), round);

        let now = beacon.beacon_round_timestamp(round).unwrap() + 1;
        handle_drand_entry(
            entry,
            message_id,
            source,
            &limiter,
            &schedule,
            &network_send,
            now,
        );
        match tokio::time::timeout(Duration::from_secs(5), verdicts.recv_async())
            .await
            .expect("no verdict reported")
            .unwrap()
        {
            NetworkMessage::ReportValidation { acceptance, .. } => {
                assert!(
                    matches!(acceptance, MessageAcceptance::Accept),
                    "round {round}"
                );
            }
            other => panic!("unexpected network message: {other:?}"),
        }
    }

    // The beacon has no HTTP servers: every round is served from the gossip-filled cache.
    for round in 1..=5u64 {
        assert_eq!(beacon.entry(round).await.unwrap(), drand.entry(round));
    }
}

async fn wait_until_meshed(
    node: &mut Swarm<Gossipsub>,
    relay: &mut Swarm<gossipsub::Behaviour>,
    topic: &IdentTopic,
) {
    let hash = topic.hash();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if relay.behaviour().mesh_peers(&hash).next().is_some()
                && node.behaviour().mesh_peers(&hash).next().is_some()
            {
                return;
            }

            tokio::select! {
                _ = node.select_next_some() => {}
                _ = relay.select_next_some() => {}
            }
        }
    })
    .await
    .expect("drand topic mesh never formed");
}

#[tokio::test]
async fn cache_miss_falls_back_to_http_once() {
    use axum::{Json, Router, extract::Path, routing::get};
    use std::sync::atomic::{AtomicUsize, Ordering};

    // mocks drand HTTP
    let hits = Arc::new(AtomicUsize::new(0));
    let signer = Arc::new(FakeDrand::new(
        vec![],
        FAKE_DRAND_PERIOD,
        FAKE_DRAND_GENESIS_TIME,
    ));

    let app = {
        let (hits, signer) = (hits.clone(), signer.clone());
        Router::new().route(
            "/{hash}/public/{round}",
            get(move |Path((_hash, round)): Path<(String, u64)>| {
                let (hits, signer) = (hits.clone(), signer.clone());
                async move {
                    hits.fetch_add(1, Ordering::Relaxed);
                    Json(signer.to_json(round))
                }
            }),
        )
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base: url::Url = format!("http://{}/", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let drand = FakeDrand::new(vec![base], FAKE_DRAND_PERIOD, FAKE_DRAND_GENESIS_TIME);
    let beacon = drand.beacon(TEST_FIL_GENESIS_TIME, TEST_FIL_BLOCK_DELAY);

    // sanity check
    assert_eq!(hits.load(Ordering::Relaxed), 0);

    // first fetch
    let fetched = beacon.entry(42).await.unwrap();
    assert_eq!(fetched.round(), 42);
    assert_eq!(hits.load(Ordering::Relaxed), 1, "expected one HTTP fetch");

    // second call, same round, should fetch from cache
    beacon.entry(42).await.unwrap();
    assert_eq!(
        hits.load(Ordering::Relaxed),
        1,
        "second call must not reach HTTP"
    );
}

fn gossip_message_event(data: Vec<u8>, topic: gossipsub::TopicHash) -> gossipsub::Event {
    gossipsub::Event::Message {
        propagation_source: libp2p::PeerId::random(),
        message_id: gossipsub::MessageId::new(b"test"),
        message: gossipsub::Message {
            source: None,
            data,
            sequence_number: None,
            topic,
        },
    }
}

fn drand_topic_kinds(
    drand: &FakeDrand,
) -> (
    IdentTopic,
    ahash::HashMap<gossipsub::TopicHash, PubsubTopic>,
) {
    let topic = IdentTopic::new(format!("{PUBSUB_DRAND_STR}/{}", drand.chain_info_hash()));
    let mut kinds = ahash::HashMap::default();
    kinds.insert(topic.hash(), PubsubTopic::Drand);
    (topic, kinds)
}

#[tokio::test]
async fn gossip_drand_message_is_decoded_and_emitted() {
    let drand = FakeDrand::new(vec![], FAKE_DRAND_PERIOD, FAKE_DRAND_GENESIS_TIME);
    let (topic, kinds) = drand_topic_kinds(&drand);
    let (tx, rx) = flume::unbounded();

    // The payload is a bare `PublicRandResponse`, no length prefix (regression:
    // decoding used to assume a prefix and reject every live relay message).
    let verdict = handle_gossip_event(
        gossip_message_event(drand.to_protobuf(42), topic.hash()),
        &tx,
        &kinds,
    )
    .await;
    assert!(
        verdict.is_none(),
        "drand verdicts are deferred to the chain follower"
    );

    match rx.try_recv().expect("no event emitted") {
        NetworkEvent::PubsubMessage {
            message: PubsubMessage::DrandEntry { entry, .. },
        } => {
            assert_eq!(entry.round(), 42);
            assert_eq!(entry.signature(), drand.entry(42).signature());
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[tokio::test]
async fn gossip_drand_malformed_payload_is_dropped() {
    let drand = FakeDrand::new(vec![], FAKE_DRAND_PERIOD, FAKE_DRAND_GENESIS_TIME);
    let (topic, kinds) = drand_topic_kinds(&drand);
    let (tx, rx) = flume::unbounded();

    let verdict = handle_gossip_event(
        gossip_message_event(vec![0xff, 0xff, 0xff], topic.hash()),
        &tx,
        &kinds,
    )
    .await;
    assert!(matches!(verdict, Some((_, _, MessageAcceptance::Reject))));

    assert!(
        rx.try_recv().is_err(),
        "malformed payload must emit nothing"
    );
}

#[tokio::test]
async fn gossip_message_on_unknown_topic_is_dropped() {
    let drand = FakeDrand::new(vec![], FAKE_DRAND_PERIOD, FAKE_DRAND_GENESIS_TIME);
    let (_, kinds) = drand_topic_kinds(&drand);
    let (tx, rx) = flume::unbounded();

    let verdict = handle_gossip_event(
        gossip_message_event(
            drand.to_protobuf(1),
            gossipsub::TopicHash::from_raw("/unknown/topic"),
        ),
        &tx,
        &kinds,
    )
    .await;
    assert!(matches!(verdict, Some((_, _, MessageAcceptance::Ignore))));

    assert!(rx.try_recv().is_err(), "unknown topic must emit nothing");
}
