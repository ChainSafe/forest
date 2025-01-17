// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::go_ffi::*;
use cid::Cid;
use forest::interop_tests_private::libp2p_bitswap::{
    BitswapBehaviour, BitswapBehaviourEvent, BitswapMessage, BitswapRequest, BitswapResponse,
};
use libp2p::{
    futures::StreamExt as _, noise, request_response, swarm::SwarmEvent, tcp, yamux, Multiaddr,
    Swarm, SwarmBuilder,
};
use multihash_codetable::{Code, MultihashDigest as _};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(60);
const LISTEN_ADDR: &str = "/ip4/127.0.0.1/tcp/0";

#[tokio::test(flavor = "multi_thread")]
async fn bitswap_go_compat_test() {
    bitswap_go_compat_test_impl().await.unwrap()
}

async fn bitswap_go_compat_test_impl() -> anyhow::Result<()> {
    let (mut swarm, listen_addr) = create_node().await?;

    let expected_inbound_request_cid_str = "bitswap_request_from_go";
    let expected_inbound_request_cid =
        Cid::new_v0(Code::Sha2_256.digest(expected_inbound_request_cid_str.as_bytes()))?;
    let outbound_request_cid = Cid::new_v0(Code::Sha2_256.digest(b"bitswap_request_from_rust"))?;
    let (inbound_request_tx, inbound_request_rx) = flume::unbounded();
    let (inbound_response_tx, inbound_response_rx) = flume::unbounded();
    tokio::spawn(async move {
        loop {
            // Swarm event loop
            match swarm.select_next_some().await {
                SwarmEvent::Behaviour(BitswapBehaviourEvent::Message { peer, message, .. }) => {
                    let bitswap = &mut swarm.behaviour_mut();
                    match message {
                        request_response::Message::Request {
                            request_id: _,
                            request,
                            channel,
                        } => {
                            // Close the stream immediately, `go-bitswap` does not read
                            // response(s) from this stream
                            // so they will be sent over another stream
                            bitswap.inner_mut().send_response(channel, ()).unwrap();
                            for message in request {
                                match message {
                                    BitswapMessage::Request(r) => {
                                        if r.cancel {
                                            continue;
                                        }

                                        // 1. Get an inbound request from go app
                                        if r.cid == expected_inbound_request_cid {
                                            // Send a response to the go app
                                            bitswap.send_response(
                                                &peer,
                                                (
                                                    r.cid,
                                                    BitswapResponse::Block(
                                                        expected_inbound_request_cid_str
                                                            .as_bytes()
                                                            .to_vec(),
                                                    ),
                                                ),
                                            );

                                            inbound_request_tx.send_async(peer).await.unwrap();
                                            // Send a request to the go app
                                            bitswap.send_request(
                                                &peer,
                                                BitswapRequest::new_have(outbound_request_cid)
                                                    .send_dont_have(true),
                                            );
                                        } else {
                                            bitswap.send_response(
                                                &peer,
                                                (r.cid, BitswapResponse::Have(false)),
                                            );
                                        }
                                    }
                                    BitswapMessage::Response(cid, ..) => {
                                        // 2. Check inbound response
                                        if cid == outbound_request_cid {
                                            inbound_response_tx.send_async(()).await.unwrap();
                                        }
                                    }
                                }
                            }
                        }
                        request_response::Message::Response { .. } => {}
                    }
                }
                _ => {}
            }
        }
    });

    GoBitswapNodeImpl::run();
    GoBitswapNodeImpl::connect(&listen_addr.to_string());
    assert!(
        GoBitswapNodeImpl::get_block(&expected_inbound_request_cid.to_string()),
        "[Go] get_block failed"
    );

    // 1. Receive request from `go-bitswap`
    tokio::time::timeout(TIMEOUT, inbound_request_rx.recv_async()).await??;
    println!("Received request from go-bitswap test app");
    // 2. Receive response from `go-bitswap`
    tokio::time::timeout(TIMEOUT, inbound_response_rx.recv_async()).await??;
    println!("Received response from go-bitswap test app");

    Ok(())
}

async fn create_node() -> anyhow::Result<(Swarm<BitswapBehaviour>, Multiaddr)> {
    let mut swarm = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default().nodelay(true),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|_keypair| {
            BitswapBehaviour::new(&["/test/ipfs/bitswap/1.2.0"], Default::default())
        })?
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
