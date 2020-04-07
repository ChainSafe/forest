// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_blocks::{Block, BlockHeader, FullTipset};
use forest_cid::Cid;
use forest_encoding::{
    de::{self, Deserialize, Deserializer},
    ser::{self, Serialize, Serializer},
};
use forest_message::{SignedMessage, UnsignedMessage};
use std::convert::TryFrom;

/// Blocksync request options
pub const BLOCKS: u64 = 1;
pub const MESSAGES: u64 = 2;

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

impl BlockSyncResponse {
    pub fn into_result(self) -> Result<Vec<FullTipset>, String> {
        if self.status != 0 {
            // TODO implement a better error type than string if needed to be handled differently
            return Err(format!("Status {}: {}", self.status, self.message));
        }

        Ok(self
            .chain
            .into_iter()
            .map(FullTipset::try_from)
            .collect::<Result<_, _>>()?)
    }
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

impl TryFrom<TipSetBundle> for FullTipset {
    type Error = &'static str;

    fn try_from(tsb: TipSetBundle) -> Result<FullTipset, Self::Error> {
        let mut blocks: Vec<Block> = Vec::with_capacity(tsb.blocks.len());

        if tsb.blocks.len() != tsb.bls_msg_includes.len()
            || tsb.blocks.len() != tsb.secp_msg_includes.len()
        {
            return Err("Invalid formed TipSet bundle, lengths of includes does not match blocks");
        }

        fn values_from_indexes<T: Clone>(
            indexes: &[u64],
            values: &[T],
        ) -> Result<Vec<T>, &'static str> {
            let mut msgs = Vec::with_capacity(indexes.len());
            for idx in indexes.iter() {
                msgs.push(
                    values
                        .get(*idx as usize)
                        .cloned()
                        .ok_or_else(|| "Invalid message index")?,
                );
            }
            Ok(msgs)
        }

        for (i, header) in tsb.blocks.into_iter().enumerate() {
            let bls_messages = values_from_indexes(&tsb.bls_msg_includes[i], &tsb.bls_msgs)?;
            let secp_messages = values_from_indexes(&tsb.secp_msg_includes[i], &tsb.secp_msgs)?;

            blocks.push(Block {
                header,
                secp_messages,
                bls_messages,
            });
        }

        // TODO FullTipset constructor doesn't perform any validation (but probably should?)
        Ok(FullTipset::new(blocks))
    }
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
