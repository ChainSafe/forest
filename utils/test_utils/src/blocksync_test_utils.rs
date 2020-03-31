// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod chain_structures;

use self::chain_structures::*;
use forest_libp2p::blocksync::{BlockSyncRequest, BlockSyncResponse};
use forest_libp2p::rpc::{RPCEvent, RPCMessage, RPCRequest, RPCResponse};
use forest_libp2p::test::rpc_test_utils::*;
use futures::future;


pub fn init_blocksync_response() {
    let (_, mut receiver) = build_node_pair();

    let headers = header_setup(3);
    let (bls, secp) = block_msgs_setup();
    
    let rpc_response = RPCResponse::BlockSync(BlockSyncResponse {
        chain: TipSetBundle {
            blocks: headers,
            bls_msgs: bls,
            secp_msgs: secp,
            bls_msg_includes: vec![],
            secp_msg_includes: vec![],
        },
        status: 1,
        message: "message".to_owned(),
    });
    let c_response = rpc_response.clone();

        let receiver_fut = async move {
            loop {
                match receiver.next().await {
                    RPCMessage::RPC(source, event) => {
                        match event {
                            RPCEvent::Request(req_id, req) => {
                                assert_eq!(rpc_request.clone(), req);
                                assert_eq!(req_id, 1);
                                // send the response
                                receiver
                                    .send_rpc(peer_id, RPCEvent::Response(1, c_response.clone()));
                            }
                            ev => panic!("Receiver invalid RPC received, {:?}", ev),
                        }
                    }
                    e => panic!("unexpected {:?}", e),
                }
            }
        };

        let result = future::select(Box::pin(receiver_fut));
        let ((req_id, res), _) = async_std::task::block_on(result).factor_first();
        assert_eq!(res, rpc_response);
}