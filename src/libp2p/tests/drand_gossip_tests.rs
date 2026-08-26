use std::{sync::Arc, time::Duration};

use futures::StreamExt as _;
use libp2p::{
    Swarm,
    gossipsub::{self, IdentTopic},
    swarm::SwarmEvent,
};
use libp2p_swarm_test::SwarmExt as _;
use quick_protobuf::{BytesReader, MessageRead};

use crate::libp2p::{PUBSUB_DRAND_STR, build_gossipsub};
use crate::networks::GenesisNetworkName;
use crate::{
    beacon::{
        Beacon, BeaconEntry, PublicRandResponse,
        tests::fake_drand::{
            FAKE_DRAND_GENESIS_TIME, FAKE_DRAND_PERIOD, FakeDrand, TEST_FIL_BLOCK_DELAY,
            TEST_FIL_GENESIS_TIME,
        },
    },
    libp2p::{Gossipsub, PubsubTopicCfg},
};

#[tokio::test]
async fn gossip_rounds_are_verified_and_cached() {
    let drand = FakeDrand::new(vec![], FAKE_DRAND_PERIOD, FAKE_DRAND_GENESIS_TIME);

    let beacon = drand.beacon(TEST_FIL_GENESIS_TIME, TEST_FIL_BLOCK_DELAY);
    let hash = drand.chain_info_hash();

    let topic = IdentTopic::new(format!("{PUBSUB_DRAND_STR}/{hash}"));

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

    let mut received = Vec::new();
    for round in 1..=5u64 {
        relay
            .behaviour_mut()
            .publish(topic.clone(), drand.to_protobuf(round))
            .unwrap();
        let data = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                tokio::select! {
                    _ = relay.select_next_some() => {},
                    ev = node.select_next_some() => {
                        if let SwarmEvent::Behaviour(gossipsub::Event::Message { message, .. }) = ev {
                            break message.data;
                        }
                    }
                }
            }
        }).await.expect("no gossip message");

        let mut reader = BytesReader::from_bytes(&data);
        let decoded = PublicRandResponse::from_reader(&mut reader, &data).unwrap();
        received.push(BeaconEntry::new(decoded.round, decoded.signature));
    }

    assert_eq!(received.len(), 5);
    assert!(
        beacon
            .verify_entries(&received, &BeaconEntry::default())
            .unwrap()
    );

    // verify every round is now served from cache.
    for round in 1..=5u64 {
        assert_eq!(beacon.entry(round).await.unwrap().round(), round);
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
async fn silence_past_deadline_fallback_to_http() {
    use axum::{routing::get, Router, extract::Path, Json};
    use std::sync::atomic::{AtomicUsize, Ordering};

    // mocks drand HTTP
    let hits = Arc::new(AtomicUsize::new(0));
    let signer = Arc::new(FakeDrand::new(vec![], FAKE_DRAND_PERIOD, FAKE_DRAND_GENESIS_TIME));

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
            })
        )
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base: url::Url = format!("http://{}/", listener.local_addr().unwrap()).parse().unwrap();
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
    assert_eq!(hits.load(Ordering::Relaxed), 1, "second call must not reach HTTP");
}