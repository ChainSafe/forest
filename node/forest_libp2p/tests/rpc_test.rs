// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(test)]

use async_std::task;
use forest_libp2p::rpc::{
    BlockSyncRequest, BlockSyncResponse, RPCEvent, RPCMessage, RPCRequest, RPCResponse, RPC,
};
use futures::{future, prelude::*};
use libp2p::core::{
    identity,
    multiaddr::Protocol,
    muxing::StreamMuxerBox,
    nodes::Substream,
    transport::{boxed::Boxed, MemoryTransport},
    upgrade, Multiaddr, PeerId, Transport,
};
use libp2p::plaintext::PlainText2Config;
use libp2p::swarm::Swarm;
use libp2p::yamux;
use std::{io::Error, task::Poll};

fn build_node_pair() -> (TestSwarm, TestSwarm) {
    let (_, mut s1) = build_node(10005);
    let (mut a2, s2) = build_node(10006);

    let _ = a2.pop();
    // dial each other
    Swarm::dial_addr(&mut s1, a2).unwrap();

    (s1, s2)
}

type TestSwarm = Swarm<Boxed<(PeerId, StreamMuxerBox), Error>, RPC<Substream<StreamMuxerBox>>>;
fn build_node(port: u64) -> (Multiaddr, TestSwarm) {
    let key = identity::Keypair::generate_ed25519();
    let public_key = key.public();

    let transport = MemoryTransport::default()
        .upgrade(upgrade::Version::V1)
        .authenticate(PlainText2Config {
            local_public_key: public_key.clone(),
        })
        .multiplex(yamux::Config::default())
        .map(|(p, m), _| (p, StreamMuxerBox::new(m)))
        .map_err(|e| panic!("Failed to create transport: {:?}", e))
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

#[test]
fn test_empty_rpc() {
    let (mut sender, mut receiver) = build_node_pair();

    let rpc_request = RPCRequest::Blocksync(BlockSyncRequest {
        start: vec![],
        request_len: 0,
        options: 0,
    });

    let rpc_response = RPCResponse::Blocksync(BlockSyncResponse {
        chain: vec![],
        status: 1,
        message: "message".to_owned(),
    });

    let rpc_poll = future::poll_fn(move |cx| -> Poll<Result<(), String>> {
        loop {
            // Poll sender swarm
            match sender.poll_next_unpin(cx) {
                Poll::Ready(Some(RPCMessage::PeerDialed(peer_id))) => {
                    // Send a BlocksByRange request
                    sender.send_rpc(peer_id, RPCEvent::Request(1, rpc_request.clone()));
                }
                Poll::Ready(Some(RPCMessage::RPC(_peer_id, event))) => match event {
                    RPCEvent::Response(req_id, res) => {
                        assert_eq!(res, rpc_response.clone());
                        assert_eq!(req_id, 1);
                        return Poll::Ready(Ok(()));
                    }
                    ev => panic!("Sender invalid RPC received, {:?}", ev),
                },
                _ => (),
            }
            // Poll receiver swarm
            match receiver.poll_next_unpin(cx) {
                Poll::Ready(Some(RPCMessage::RPC(peer_id, event))) => {
                    match event {
                        RPCEvent::Request(req_id, req) => {
                            assert_eq!(rpc_request.clone(), req);
                            assert_eq!(req_id, 1);
                            // send the response
                            receiver.send_rpc(peer_id, RPCEvent::Response(1, rpc_response.clone()));
                        }
                        ev => panic!("Receiver invalid RPC received, {:?}", ev),
                    }
                }
                _ => (),
            }
        }
    });

    // Unwrap future result, should wait until true result
    task::block_on(rpc_poll).unwrap();
}
