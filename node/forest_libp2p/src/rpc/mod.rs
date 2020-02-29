// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod behaviour;
mod codec;
mod handler;
mod protocol;

pub use behaviour::*;
pub use codec::*;
pub use handler::*;
pub use protocol::*;

use forest_encoding::error::Error as EncodingError;
use std::fmt;

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

#[derive(Debug, Clone, PartialEq)]
pub enum RPCError {
    Codec(String),
    Custom(String),
}

impl From<std::io::Error> for RPCError {
    fn from(err: std::io::Error) -> Self {
        Self::Custom(err.to_string())
    }
}

impl From<EncodingError> for RPCError {
    fn from(err: EncodingError) -> Self {
        Self::Codec(err.to_string())
    }
}

impl fmt::Display for RPCError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RPCError::Codec(err) => write!(f, "Codec Error: {}", err),
            RPCError::Custom(err) => write!(f, "{}", err),
        }
    }
}

impl std::error::Error for RPCError {
    fn description(&self) -> &str {
        "Libp2p RPC Error"
    }
}
