// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::message::SignedMessage;
use crate::shim::message::Message;
use cid::Cid;
use fvm_ipld_encoding::tuple::*;

use super::CachingBlockHeader;

/// Limit of BLS and `SECP` messages combined in a block.
pub const BLOCK_MESSAGE_LIMIT: usize = 10000;

/// A complete Filecoin block. This contains the block header as well as all BLS
/// and `SECP` messages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub header: CachingBlockHeader,
    pub bls_messages: Vec<Message>,
    pub secp_messages: Vec<SignedMessage>,
}

impl std::hash::Hash for Block {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(self.cid(), state)
    }
}

impl Block {
    pub fn header(&self) -> &CachingBlockHeader {
        &self.header
    }
    pub fn bls_msgs(&self) -> &[Message] {
        &self.bls_messages
    }
    pub fn secp_msgs(&self) -> &[SignedMessage] {
        &self.secp_messages
    }
    /// Returns block header's CID.
    pub fn cid(&self) -> &Cid {
        self.header.cid()
    }
}

/// Tracks the Merkle roots of both `SECP` and BLS messages separately.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct TxMeta {
    pub bls_message_root: Cid,
    pub secp_message_root: Cid,
}
