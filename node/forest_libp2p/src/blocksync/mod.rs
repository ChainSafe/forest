// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod message;

pub use self::message::*;
use libp2p::core::ProtocolName;

pub const BLOCKSYNC_PROTOCOL_ID: &[u8] = b"/fil/sync/blk/0.0.1";

#[derive(Clone, Debug, PartialEq, Default)]
pub struct BlockSyncProtocolName;

impl ProtocolName for BlockSyncProtocolName {
    fn protocol_name(&self) -> &[u8] {
        BLOCKSYNC_PROTOCOL_ID
    }
}
