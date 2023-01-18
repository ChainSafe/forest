// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::BlockHeader;
use cid::Cid;
use forest_encoding::tuple::*;
use forest_message::SignedMessage;
use fvm_shared::message::Message;

/// Limit of BLS and SECP messages combined in a block.
pub const BLOCK_MESSAGE_LIMIT: usize = 10000;

/// A complete Filecoin block. This contains the block header as well as all BLS and SECP messages.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub header: BlockHeader,
    pub bls_messages: Vec<Message>,
    pub secp_messages: Vec<SignedMessage>,
}

impl Block {
    /// Returns reference to the [`BlockHeader`].
    pub fn header(&self) -> &BlockHeader {
        &self.header
    }
    /// Returns reference to the block's BLS [`Message`]s.
    pub fn bls_msgs(&self) -> &[Message] {
        &self.bls_messages
    }
    /// Returns reference to the block's SECP [`SignedMessage`]s.
    pub fn secp_msgs(&self) -> &[SignedMessage] {
        &self.secp_messages
    }
    /// Returns block's `cid`. This `cid` is the same as the [`BlockHeader::cid`].
    pub fn cid(&self) -> &Cid {
        self.header.cid()
    }
}

/// Tracks the Merkle roots of both SECP and BLS messages separately.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct TxMeta {
    pub bls_message_root: Cid,
    pub secp_message_root: Cid,
}
