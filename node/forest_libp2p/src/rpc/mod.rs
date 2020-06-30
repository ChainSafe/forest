// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod codec;
mod error;
mod protocol;

pub use self::codec::{InboundCodec, OutboundCodec};
pub use self::error::RPCError;
pub use self::protocol::{RPCRequest, RPCResponse};
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
