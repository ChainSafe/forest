// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use libp2p::request_response::ProtocolName;

use super::*;
use crate::rpc::CborRequestResponse;

/// Libp2p Hello protocol ID.
pub const HELLO_PROTOCOL_ID: &[u8] = b"/fil/hello/1.0.0";

/// Type to satisfy `ProtocolName` interface for Hello RPC.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct HelloProtocolName;

impl ProtocolName for HelloProtocolName {
    fn protocol_name(&self) -> &[u8] {
        HELLO_PROTOCOL_ID
    }
}

/// Hello protocol codec to be used within the RPC service.
pub type HelloCodec = CborRequestResponse<HelloProtocolName, HelloRequest, HelloResponse>;
