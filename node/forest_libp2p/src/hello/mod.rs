// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod message;

pub use self::message::*;
use libp2p::core::ProtocolName;

pub const HELLO_PROTOCOL_ID: &[u8] = b"/fil/hello/1.0.0";

#[derive(Clone, Debug, PartialEq, Default)]
pub struct HelloProtocolName;

impl ProtocolName for HelloProtocolName {
    fn protocol_name(&self) -> &[u8] {
        HELLO_PROTOCOL_ID
    }
}
