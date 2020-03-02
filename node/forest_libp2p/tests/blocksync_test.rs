// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod rpc_test_utils;

use self::rpc_test_utils::*;
use forest_libp2p::blocksync::{BlockSyncRequest, BlockSyncResponse};
use forest_libp2p::rpc::{RPCEvent, RPCMessage, RPCRequest, RPCResponse};
use futures::future;

#[test]
fn test_empty_blocksync() {
    let (mut sender, mut receiver) = build_node_pair();

    let rpc_request = RPCRequest::BlockSync(BlockSyncRequest {
        start: vec![],
        request_len: 0,
        options: 0,
    });

    let c_request = rpc_request.clone();
    let rpc_response = RPCResponse::BlockSync(BlockSyncResponse {
        chain: vec![],
        status: 1,
        message: "message".to_owned(),
    });
    let c_response = rpc_response.clone();

    let sender_fut = async move {
        loop {
            match sender.next().await {
                RPCMessage::PeerDialed(peer_id) => {
                    // Send a BlocksByRange request
                    sender.send_rpc(peer_id, RPCEvent::Request(1, c_request.clone()));
                }
                RPCMessage::RPC(_peer_id, event) => match event {
                    RPCEvent::Response(req_id, res) => {
                        return (req_id, res);
                    }
                    ev => panic!("Sender invalid RPC received, {:?}", ev),
                },
                e => panic!("unexpected {:?}", e),
            }
        }
    };

    let receiver_fut = async move {
        loop {
            match receiver.next().await {
                RPCMessage::RPC(peer_id, event) => {
                    match event {
                        RPCEvent::Request(req_id, req) => {
                            assert_eq!(rpc_request.clone(), req);
                            assert_eq!(req_id, 1);
                            // send the response
                            receiver.send_rpc(peer_id, RPCEvent::Response(1, c_response.clone()));
                        }
                        ev => panic!("Receiver invalid RPC received, {:?}", ev),
                    }
                }
                e => panic!("unexpected {:?}", e),
            }
        }
    };

    let result = future::select(Box::pin(sender_fut), Box::pin(receiver_fut));
    let ((req_id, res), _) = async_std::task::block_on(result).factor_first();
    assert_eq!(res, rpc_response);
    assert_eq!(req_id, 1);
}
