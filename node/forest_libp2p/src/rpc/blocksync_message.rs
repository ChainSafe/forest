// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_blocks::BlockHeader;
use forest_cid::Cid;
use forest_encoding::{
    de::{self, Deserialize, Deserializer},
    ser::{self, Serialize, Serializer},
};
use forest_message::{SignedMessage, UnsignedMessage};

/// The payload that gets sent to another node to request for blocks and messages. It get DagCBOR serialized before sending over the wire.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockSyncRequest {
    /// The tipset to start sync from
    pub start: Vec<Cid>,
    /// The amount of epochs to sync by
    pub request_len: u64,
    /// 1 = Block only, 2 = Messages only, 3 = Blocks and Messages
    pub options: u64,
}

impl Serialize for BlockSyncRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.start, &self.request_len, &self.options).serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for BlockSyncRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (start, request_len, options) = Deserialize::deserialize(deserializer)?;
        Ok(BlockSyncRequest {
            start,
            request_len,
            options,
        })
    }
}

/// The response to a BlockSync request.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockSyncResponse {
    /// The tipsets requested
    pub chain: Vec<TipSetBundle>,
    /// Error code
    pub status: u64,
    /// Status message indicating failure reason
    // TODO not included in blocksync spec, revisit if it will be removed in future
    pub message: String,
}

impl Serialize for BlockSyncResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.chain, &self.status, &self.message).serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for BlockSyncResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (chain, status, message) = Deserialize::deserialize(deserializer)?;
        Ok(BlockSyncResponse {
            chain,
            status,
            message,
        })
    }
}

/// Contains the blocks and messages in a particular tipset
#[derive(Clone, Debug, PartialEq)]
pub struct TipSetBundle {
    /// The blocks in the tipset
    pub blocks: Vec<BlockHeader>,

    /// Signed bls messages
    pub bls_msgs: Vec<UnsignedMessage>,
    /// Describes which block each message belongs to
    pub bls_msg_includes: Vec<Vec<u64>>,

    /// Unsigned secp messages
    pub secp_msgs: Vec<SignedMessage>,
    /// Describes which block each message belongs to
    pub secp_msg_includes: Vec<Vec<u64>>,
}

impl ser::Serialize for TipSetBundle {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        (
            &self.blocks,
            &self.bls_msgs,
            &self.bls_msg_includes,
            &self.secp_msgs,
            &self.secp_msg_includes,
        )
            .serialize(serializer)
    }
}

impl<'de> de::Deserialize<'de> for TipSetBundle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (blocks, bls_msgs, bls_msg_includes, secp_msgs, secp_msg_includes) =
            Deserialize::deserialize(deserializer)?;
        Ok(TipSetBundle {
            blocks,
            bls_msgs,
            bls_msg_includes,
            secp_msgs,
            secp_msg_includes,
        })
    }
}
