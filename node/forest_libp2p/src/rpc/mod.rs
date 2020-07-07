// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocksync::{BlockSyncRequest, BlockSyncResponse};
use crate::hello::{HelloRequest, HelloResponse};
pub use libp2p_request_response::{RequestId, ResponseChannel};

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
