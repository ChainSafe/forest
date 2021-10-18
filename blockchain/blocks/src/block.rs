// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::BlockHeader;
use cid::Cid;
use encoding::tuple::*;
use message::{SignedMessage, UnsignedMessage};

/// Limit of bls and secp messages combined in a block.
pub const BLOCK_MESSAGE_LIMIT: usize = 10000;

/// A complete Filecoin block. This contains the block header as well as all bls and secp messages.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub header: BlockHeader,
    pub bls_messages: Vec<UnsignedMessage>,
    pub secp_messages: Vec<SignedMessage>,
}

impl Block {
    /// Returns reference to the [BlockHeader].
    pub fn header(&self) -> &BlockHeader {
        &self.header
    }
    /// Returns reference to the block's BLS [UnsignedMessage]s.
    pub fn bls_msgs(&self) -> &[UnsignedMessage] {
        &self.bls_messages
    }
    /// Returns reference to the block's Secp256k1 [SignedMessage]s.
    pub fn secp_msgs(&self) -> &[SignedMessage] {
        &self.secp_messages
    }
    /// Returns block's cid. This cid is the same as the [BlockHeader::cid].
    pub fn cid(&self) -> &Cid {
        self.header.cid()
    }
}

/// Tracks the merkleroots of both secp and bls messages separately.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct TxMeta {
    pub bls_message_root: Cid,
    pub secp_message_root: Cid,
}
