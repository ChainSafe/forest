// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_libp2p::rpc::RPC;
use std::io::Error;

use libp2p::core::{
    identity, multiaddr::Protocol, muxing::StreamMuxerBox, transport::MemoryTransport, upgrade,
    Multiaddr, Transport,
};
use libp2p::plaintext::PlainText2Config;
use libp2p::yamux;
use libp2p::Swarm;

pub fn build_node_pair() -> (TestSwarm, TestSwarm) {
    let (_, mut s1) = build_node(10005);
    let (mut a2, s2) = build_node(10006);

    let _ = a2.pop();
    // dial each other
    Swarm::dial_addr(&mut s1, a2).unwrap();

    (s1, s2)
}

pub type TestSwarm = Swarm<RPC>;
pub fn build_node(port: u64) -> (Multiaddr, TestSwarm) {
    let key = identity::Keypair::generate_ed25519();
    let public_key = key.public();

    let transport = MemoryTransport::default()
        .upgrade(upgrade::Version::V1)
        .authenticate(PlainText2Config {
            local_public_key: public_key.clone(),
        })
        .multiplex(yamux::Config::default())
        .map(|(p, m), _| (p, StreamMuxerBox::new(m)))
        .map_err(|e| -> Error { panic!("Failed to create transport: {:?}", e) })
        .boxed();

    let peer_id = public_key.clone().into_peer_id();
    let behaviour = RPC::new();
    let mut swarm = Swarm::new(transport, behaviour, peer_id);

    let mut addr: Multiaddr = Protocol::Memory(port).into();
    Swarm::listen_on(&mut swarm, addr.clone()).unwrap();

    addr = addr.with(libp2p::core::multiaddr::Protocol::P2p(
        public_key.into_peer_id().into(),
    ));

    (addr, swarm)
}
