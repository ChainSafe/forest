// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::BlockHeader;
use cid::Cid;
use encoding::{de::Deserializer, ser::Serializer};
use message::{SignedMessage, UnsignedMessage};
use serde::{Deserialize, Serialize};

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
pub struct TxMeta {
    pub bls_message_root: Cid,
    pub secp_message_root: Cid,
}

impl Serialize for TxMeta {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.bls_message_root, &self.secp_message_root).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TxMeta {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (bls_message_root, secp_message_root) = Deserialize::deserialize(deserializer)?;
        Ok(TxMeta {
            bls_message_root,
            secp_message_root,
        })
    }
}
