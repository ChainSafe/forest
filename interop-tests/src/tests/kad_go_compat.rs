// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::go_ffi::*;

use forest::interop_tests_private::libp2p::discovery::new_kademlia;
use futures::StreamExt as _;
use libp2p::{
    identify, identity, kad, noise, swarm::SwarmEvent, tcp, yamux, Multiaddr, StreamProtocol,
    Swarm, SwarmBuilder,
};
use libp2p_swarm_test::SwarmExt as _;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(600);
const LISTEN_ADDR: &str = "/ip4/127.0.0.1/tcp/0";

type SwarmType = Swarm<TestBehaviour>;

#[tokio::test(flavor = "multi_thread")]
async fn kad_go_compat_test() -> anyhow::Result<()> {
    let (mut swarm1, addr1) = create_node().await?;
    let (swarm2, addr2) = create_node().await?;
    swarm1
        .behaviour_mut()
        .kad
        .add_address(swarm2.local_peer_id(), addr2);

    tokio::spawn(swarm1.loop_on_next());
    tokio::spawn(swarm2.loop_on_next());

    GoKadNodeImpl::run();
    GoKadNodeImpl::connect(&addr1.to_string());
    // Wait for 10s
    for _ in 0..10 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        if GoKadNodeImpl::get_n_connected() > 2 {
            break;
        }
    }
    assert!(GoKadNodeImpl::get_n_connected() > 2);
    Ok(())
}

async fn create_node() -> anyhow::Result<(SwarmType, Multiaddr)> {
    let mut swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|keypair| TestBehaviour::new(keypair.public()))?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(TIMEOUT))
        .build();
    let local_peer_id = *swarm.local_peer_id();
    swarm.listen_on(LISTEN_ADDR.parse()?)?;
    let listen_addr = {
        loop {
            if let SwarmEvent::NewListenAddr {
                listener_id: _,
                address,
            } = swarm.select_next_some().await
            {
                break address;
            }
        }
    };

    Ok((swarm, listen_addr.with_p2p(local_peer_id).unwrap()))
}

#[derive(libp2p::swarm::NetworkBehaviour)]
#[behaviour(prelude = "libp2p::swarm::derive_prelude")]
struct TestBehaviour {
    kad: kad::Behaviour<kad::store::MemoryStore>,
    identify: identify::Behaviour,
}

impl TestBehaviour {
    fn new(local_public_key: identity::PublicKey) -> Self {
        let kad_peer_id = local_public_key.to_peer_id();
        let kad = new_kademlia(kad_peer_id, StreamProtocol::new("/kadtest/kad/1.0.0"));
        let identify = identify::Behaviour::new(
            identify::Config::new(Default::default(), local_public_key)
                .with_push_listen_addr_updates(true),
        );
        Self { kad, identify }
    }
}
