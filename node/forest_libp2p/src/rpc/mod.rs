// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocksync::{BlockSyncRequest, BlockSyncResponse};
use crate::hello::{HelloRequest, HelloResponse};
pub use libp2p::request_response::{RequestId, ResponseChannel};

/// The return type used in the behaviour and the resultant event from the protocols handler.
#[derive(Debug)]
pub enum RPCEvent {
    HelloRequest {
        request: HelloRequest,
        channel: ResponseChannel<HelloResponse>,
    },
    HelloResponse {
        request_id: RequestId,
        response: HelloResponse,
    },
    BlockSyncRequest {
        request: BlockSyncRequest,
        channel: ResponseChannel<BlockSyncResponse>,
    },
    BlockSyncResponse {
        request_id: RequestId,
        response: BlockSyncResponse,
    },
}

/// RPCResponse payloads for request/response calls
#[derive(Debug, Clone, PartialEq)]
pub enum RPCResponse {
    BlockSync(BlockSyncResponse),
    Hello(HelloResponse),
}

/// RPCRequest payloads for request/response calls
#[derive(Debug, Clone, PartialEq)]
pub enum RPCRequest {
    BlockSync(BlockSyncRequest),
    Hello(HelloRequest),
}
