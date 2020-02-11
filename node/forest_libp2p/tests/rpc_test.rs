// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(test)]

use async_std::task;
use forest_libp2p::rpc::{Message, RPCEvent, RPCRequest, RPCResponse, Response};
use forest_libp2p::ForestBehaviourEvent;
use forest_libp2p::Libp2pConfig;
use forest_libp2p::{Libp2pService, NetworkMessage};
use futures::future;
use futures::prelude::*;
use libp2p::identity::Keypair;
use libp2p::swarm::Swarm;
use slog::{o, warn, Drain};
use slog_async;
use slog_term;
use std::task::Poll;

pub fn setup_logging() -> slog::Logger {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();

    slog::Logger::root(drain, o!())
}

fn build_node_pair() -> (Libp2pService, Libp2pService) {
    let log = setup_logging();
    let mut config1 = Libp2pConfig::default();
    let mut config2 = Libp2pConfig::default();
    config1.listening_multiaddr = "/ip4/0.0.0.0/tcp/10005".to_owned();
    config2.listening_multiaddr = "/ip4/0.0.0.0/tcp/10006".to_owned();

    let lp2p_service1 = Libp2pService::new(log.clone(), &config1, Keypair::generate_ed25519());
    let mut lp2p_service2 = Libp2pService::new(log.clone(), &config2, Keypair::generate_ed25519());

    // dial each other
    Swarm::dial_addr(
        &mut lp2p_service2.swarm,
        "/ip4/127.0.0.1/tcp/10005".parse().unwrap(),
    )
    .unwrap();

    (lp2p_service1, lp2p_service2)
}

#[test]
fn test1() {
    let (mut sender, mut receiver) = build_node_pair();
    // let sen_tx = sender.pubsub_sender();
    // let _sen_rx = sender.pubsub_receiver();
    // let rec_tx = sender.pubsub_sender();
    // let rec_rx = sender.pubsub_receiver();

    let rpc_request = RPCEvent::Request(
        1,
        RPCRequest::Blocksync(Message {
            start: vec![],
            request_len: 0,
            options: 0,
        }),
    );

    let rpc_response = Response {
        chain: vec![],
        status: 1,
        message: "message".to_owned(),
    };

    let _rpc_msg = NetworkMessage::RPCRequest {
        peer_id: Swarm::local_peer_id(&receiver.swarm).clone(),
        request: rpc_request.clone(),
    };

    let rpc_poll = future::poll_fn(move |cx| -> Poll<Result<(), String>> {
        // Poll sender swarm
        match sender.swarm.poll_next_unpin(cx) {
            // TODO catch a dialed peer event to send request here instead
            Poll::Ready(Some(ForestBehaviourEvent::DiscoveredPeer(_peer_id))) => {
                // Send a BlocksByRange request
                warn!(sender.log, "Sender sending RPC request");
                // TODO send rpc request here
                // sender.pubsub_sender().send(rpc_request.clone());
                return Poll::Ready(Err("Temporary failure".to_owned()));
            }
            Poll::Ready(Some(ForestBehaviourEvent::RPCResponse { response, req_id })) => {
                warn!(sender.log, "Sender received a response");
                assert_eq!(req_id, 1);
                match response {
                    RPCResponse::SuccessBlocksync(res) => {
                        assert_eq!(res, rpc_response.clone());
                        // *messages_received.lock().unwrap() += 1;
                        warn!(sender.log, "Received response");
                        return Poll::Ready(Ok(()));
                    }
                    // RPCResponse::StreamTermination(ResponseTermination::BlocksByRange) => {
                    //     // should be exactly 1 messages before terminating
                    //     assert_eq!(*messages_received.lock().unwrap(), 1);
                    //     // end the test
                    //     return Poll::Ready(true);
                    // }
                    // _ => panic!("Invalid RPC received"),
                }
            }
            Poll::Ready(Some(_)) => {}
            Poll::Ready(None) | Poll::Pending => (),
        }
        // Poll receiver swarm
        match receiver.swarm.poll_next_unpin(cx) {
            Poll::Ready(Some(ForestBehaviourEvent::RPCRequest { request, req_id })) => {
                assert_eq!(req_id, 1);
                match request {
                    // Should receive the sent RPC request
                    RPCRequest::Blocksync(_request) => {
                        // TODO assert request here
                        // assert_eq!(rpc_request.clone(), request);

                        // send the response
                        warn!(receiver.log, "Receiver got request");

                        // TODO send response back here
                        // receiver.swarm.send_rpc(
                        //     peer_id.clone(),
                        //     RPCEvent::Response(
                        //         id,
                        //         RPCErrorResponse::Success(rpc_response.clone()),
                        //     ),
                        // );
                    } // _ => panic!("Received invalid RPC message"),
                }
            }
            Poll::Ready(Some(_)) => (),
            Poll::Ready(None) | Poll::Pending => (),
        }

        Poll::Pending
    });

    // Unwrap future result, should wait until true result
    task::block_on(rpc_poll).unwrap();
}
