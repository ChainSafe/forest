// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::BlockHeader;
use cid::Cid;
use encoding::tuple::*;
use message::{SignedMessage, UnsignedMessage};

/// A complete block
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub header: BlockHeader,
    pub bls_messages: Vec<UnsignedMessage>,
    pub secp_messages: Vec<SignedMessage>,
}

impl Block {
    /// Returns reference to BlockHeader
    pub fn header(&self) -> &BlockHeader {
        &self.header
    }
    /// Returns reference to unsigned messages
    pub fn bls_msgs(&self) -> &[UnsignedMessage] {
        &self.bls_messages
    }
    /// Returns reference to signed Secp256k1 messages
    pub fn secp_msgs(&self) -> &[SignedMessage] {
        &self.secp_messages
    }
    /// Returns cid for block from header
    pub fn cid(&self) -> &Cid {
        self.header.cid()
    }
}

/// Tracks the merkleroots of both secp and bls messages separately
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct TxMeta {
    pub bls_message_root: Cid,
    pub secp_message_root: Cid,
}
