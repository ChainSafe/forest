// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_blocks::{Block, BlockHeader, FullTipset, Tipset, BLOCK_MESSAGE_LIMIT};
use forest_cid::Cid;
use forest_encoding::tuple::*;
use forest_message::{SignedMessage, UnsignedMessage};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::convert::TryFrom;

/// Blocksync request options
pub const BLOCKS: u64 = 1;
pub const MESSAGES: u64 = 2;
pub const BLOCKS_MESSAGES: u64 = 3;

/// The payload that gets sent to another node to request for blocks and messages. It get DagCBOR serialized before sending over the wire.
#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct BlockSyncRequest {
    /// The tipset to start sync from
    pub start: Vec<Cid>,
    /// The amount of epochs to sync by
    pub request_len: u64,
    /// 1 = Block only, 2 = Messages only, 3 = Blocks and Messages
    pub options: u64,
}

impl BlockSyncRequest {
    /// If a request expects blocks to be included in response.
    pub fn include_blocks(&self) -> bool {
        self.options == BLOCKS || self.options == BLOCKS_MESSAGES
    }

    /// If a request expects messages to be included in response.
    pub fn include_messages(&self) -> bool {
        self.options == MESSAGES || self.options == BLOCKS_MESSAGES
    }
}

/// Status codes of a blocksync response.
#[derive(Clone, Debug, PartialEq)]
pub enum BlockSyncResponseStatus {
    /// All is well.
    Success,
    /// We could not fetch all blocks requested (but at least we returned
    /// the `Head` requested). Not considered an error.
    PartialResponse,
    /// Request.Start not found.
    BlockNotFound,
    /// Requester is making too many requests.
    GoAway,
    /// Internal error occured.
    InternalError,
    /// Request was bad
    BadRequest,
    /// Other undefined response code.
    Other(i32),
}

impl Serialize for BlockSyncResponseStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use BlockSyncResponseStatus::*;
        let code: i32 = match self {
            Success => 1,
            PartialResponse => 101,
            BlockNotFound => 201,
            GoAway => 202,
            InternalError => 203,
            BadRequest => 204,
            Other(i) => *i,
        };
        code.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BlockSyncResponseStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        let code: i32 = Deserialize::deserialize(deserializer)?;

        use BlockSyncResponseStatus::*;
        let status = match code {
            1 => Success,
            101 => PartialResponse,
            201 => BlockNotFound,
            202 => GoAway,
            203 => InternalError,
            204 => BadRequest,
            x => Other(x),
        };
        Ok(status)
    }
}

/// The response to a BlockSync request.
#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct BlockSyncResponse {
    /// Error code
    pub status: BlockSyncResponseStatus,
    /// Status message indicating failure reason
    pub message: String,
    /// The tipsets requested
    pub chain: Vec<TipsetBundle>,
}

impl BlockSyncResponse {
    /// Converts blocksync response into result.
    /// Returns an error if the response status is not `Ok`.
    /// Tipset bundle is converted into generic return type with `TryFrom` trait impl.
    pub fn into_result<T>(self) -> Result<Vec<T>, String>
    where
        T: TryFrom<TipsetBundle, Error = String>,
    {
        if self.status != BlockSyncResponseStatus::Success {
            // TODO implement a better error type than string if needed to be handled differently
            return Err(format!("Status {:?}: {}", self.status, self.message));
        }

        self.chain.into_iter().map(T::try_from).collect()
    }
}
/// Contains all bls and secp messages and their indexes per block
#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct CompactedMessages {
    /// Unsigned bls messages
    pub bls_msgs: Vec<UnsignedMessage>,
    /// Describes which block each message belongs to
    pub bls_msg_includes: Vec<Vec<u64>>,

    /// Signed secp messages
    pub secp_msgs: Vec<SignedMessage>,
    /// Describes which block each message belongs to
    pub secp_msg_includes: Vec<Vec<u64>>,
}

/// Contains the blocks and messages in a particular tipset
#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple, Default)]
pub struct TipsetBundle {
    /// The blocks in the tipset
    pub blocks: Vec<BlockHeader>,

    /// Compressed messages format
    pub messages: Option<CompactedMessages>,
}

impl TryFrom<TipsetBundle> for Tipset {
    type Error = String;

    fn try_from(tsb: TipsetBundle) -> Result<Tipset, Self::Error> {
        Tipset::new(tsb.blocks).map_err(|e| e.to_string())
    }
}

impl TryFrom<TipsetBundle> for CompactedMessages {
    type Error = String;

    fn try_from(tsb: TipsetBundle) -> Result<Self, Self::Error> {
        tsb.messages
            .ok_or_else(|| "Request contained no messages".to_string())
    }
}

impl TryFrom<TipsetBundle> for FullTipset {
    type Error = String;

    fn try_from(tsb: TipsetBundle) -> Result<FullTipset, Self::Error> {
        fts_from_bundle_parts(tsb.blocks, tsb.messages.as_ref())
    }
}

impl TryFrom<&TipsetBundle> for FullTipset {
    type Error = String;

    fn try_from(tsb: &TipsetBundle) -> Result<FullTipset, Self::Error> {
        fts_from_bundle_parts(tsb.blocks.clone(), tsb.messages.as_ref())
    }
}

fn fts_from_bundle_parts(
    headers: Vec<BlockHeader>,
    messages: Option<&CompactedMessages>,
) -> Result<FullTipset, String> {
    let CompactedMessages {
        bls_msgs,
        bls_msg_includes,
        secp_msg_includes,
        secp_msgs,
    } = messages.ok_or("Tipset bundle did not contain message bundle")?;

    // TODO: we may already want to check this on construction of the bundle
    if headers.len() != bls_msg_includes.len() || headers.len() != secp_msg_includes.len() {
        return Err(
            format!("Invalid formed Tipset bundle, lengths of includes does not match blocks. Header len: {}, bls_msg len: {}, secp_msg len: {}", headers.len(), bls_msg_includes.len(), secp_msg_includes.len()),
        );
    }

    fn values_from_indexes<T: Clone>(indexes: &[u64], values: &[T]) -> Result<Vec<T>, String> {
        indexes
            .iter()
            .map(|idx| {
                values
                    .get(*idx as usize)
                    .cloned()
                    .ok_or_else(|| "Invalid message index".to_string())
            })
            .collect()
    }

    let blocks = headers
        .into_iter()
        .enumerate()
        .map(|(i, header)| {
            let message_count = bls_msg_includes[i].len() + secp_msg_includes[i].len();
            if message_count > BLOCK_MESSAGE_LIMIT {
                return Err(format!(
                    "Block {} in bundle has too many messages ({} > {})",
                    i, message_count, BLOCK_MESSAGE_LIMIT
                ));
            }
            let bls_messages = values_from_indexes(&bls_msg_includes[i], &bls_msgs)?;
            let secp_messages = values_from_indexes(&secp_msg_includes[i], &secp_msgs)?;

            Ok(Block {
                header,
                secp_messages,
                bls_messages,
            })
        })
        .collect::<Result<_, _>>()?;

    Ok(FullTipset::new(blocks).map_err(|e| e.to_string())?)
}
