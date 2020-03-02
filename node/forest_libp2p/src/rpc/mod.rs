// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod behaviour;
mod codec;
mod error;
mod handler;
mod protocol;

pub use self::behaviour::{RPCMessage, RPC};
pub use self::codec::{InboundCodec, OutboundCodec};
pub use self::error::RPCError;
pub use self::handler::{RPCHandler, RESPONSE_TIMEOUT};
pub use self::protocol::{RPCRequest, RPCResponse};

pub type RequestId = usize;

/// The return type used in the behaviour and the resultant event from the protocols handler.
#[derive(Debug, Clone, PartialEq)]
pub enum RPCEvent {
    /// An inbound/outbound request for RPC protocol. The first parameter is a sequential
    /// id which tracks an awaiting substream for the response.
    Request(RequestId, RPCRequest),
    /// A response that is being sent or has been received from the RPC protocol. The first parameter returns
    /// that which was sent with the corresponding request, the second is a single chunk of a
    /// response.
    Response(RequestId, RPCResponse),
    /// Error in RPC request
    Error(RequestId, RPCError),
}

impl RPCEvent {
    /// Returns the id which is used to track the substream
    pub fn id(&self) -> usize {
        match *self {
            RPCEvent::Request(id, _) => id,
            RPCEvent::Response(id, _) => id,
            RPCEvent::Error(id, _) => id,
        }
    }
}

impl std::fmt::Display for RPCEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RPCEvent::Request(id, _) => write!(f, "RPC Request(id: {:?})", id),
            RPCEvent::Response(id, _) => write!(f, "RPC Response(id: {:?})", id),
            RPCEvent::Error(_, err) => write!(f, "RPC Error(error: {:?})", err),
        }
    }
}
