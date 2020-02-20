// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_blocks::BlockHeader;
use forest_cid::Cid;
use forest_encoding::{
    de::{self, Deserialize, Deserializer},
    ser::{self, Serialize, Serializer},
};
use forest_message::{SignedMessage, UnsignedMessage};

#[derive(Clone, Debug, PartialEq)]
pub struct BlockSyncRequest {
    pub start: Vec<Cid>,
    pub request_len: u64,
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

#[derive(Clone, Debug, PartialEq)]
pub struct BlockSyncResponse {
    pub chain: Vec<TipSetBundle>,
    pub status: u64,
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

#[derive(Clone, Debug, PartialEq)]
pub struct TipSetBundle {
    pub blocks: Vec<BlockHeader>,
    pub secp_msgs: Vec<UnsignedMessage>,
    pub secp_msg_includes: Vec<Vec<u64>>,

    pub bls_msgs: Vec<SignedMessage>,
    pub bls_msg_includes: Vec<Vec<u64>>,
}

impl ser::Serialize for TipSetBundle {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        (
            &self.blocks,
            &self.secp_msgs,
            &self.secp_msg_includes,
            &self.bls_msgs,
            &self.bls_msg_includes,
        )
            .serialize(serializer)
    }
}

impl<'de> de::Deserialize<'de> for TipSetBundle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (blocks, secp_msgs, secp_msg_includes, bls_msgs, bls_msg_includes) =
            Deserialize::deserialize(deserializer)?;
        Ok(TipSetBundle {
            blocks,
            secp_msgs,
            secp_msg_includes,
            bls_msgs,
            bls_msg_includes,
        })
    }
}
