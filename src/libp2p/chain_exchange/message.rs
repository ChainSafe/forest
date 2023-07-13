// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{convert::TryFrom, sync::Arc};

use crate::blocks::{Block, BlockHeader, FullTipset, Tipset, BLOCK_MESSAGE_LIMIT};
use crate::message::SignedMessage;
use crate::shim::message::Message;
use cid::Cid;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_tuple::{self, Deserialize_tuple, Serialize_tuple};

/// `ChainExchange` Filecoin header set bit.
pub const HEADERS: u64 = 0b01;
/// `ChainExchange` Filecoin messages set bit.
pub const MESSAGES: u64 = 0b10;

/// The payload that gets sent to another node to request for blocks and
/// messages.
#[derive(Clone, Debug, PartialEq, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct ChainExchangeRequest {
    /// The tipset [Cid] to start the request from.
    pub start: Vec<Cid>,
    /// The amount of epochs to request.
    pub request_len: u64,
    /// 1 for Block only, 2 for Messages only, 3 for Blocks and Messages.
    pub options: u64,
}

impl ChainExchangeRequest {
    /// If a request has the [HEADERS] bit set and requests Filecoin headers.
    pub fn include_blocks(&self) -> bool {
        self.options & HEADERS > 0
    }

    /// If a request has the [MESSAGES] bit set and requests messages of a
    /// block.
    pub fn include_messages(&self) -> bool {
        self.options & MESSAGES > 0
    }
}

/// Status codes of a `chain_exchange` response.
#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub enum ChainExchangeResponseStatus {
    /// All is well.
    Success,
    /// We could not fetch all blocks requested (but at least we returned
    /// the `Head` requested). Not considered an error.
    PartialResponse,
    /// Request.Start not found.
    BlockNotFound,
    /// Requester is making too many requests.
    GoAway,
    /// Internal error occurred.
    InternalError,
    /// Request was bad.
    BadRequest,
    /// Other undefined response code.
    Other(i32),
}

#[cfg(test)]
impl quickcheck::Arbitrary for ChainExchangeResponseStatus {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        *g.choose(&[
            ChainExchangeResponseStatus::Success,
            ChainExchangeResponseStatus::PartialResponse,
            ChainExchangeResponseStatus::BlockNotFound,
            ChainExchangeResponseStatus::GoAway,
            ChainExchangeResponseStatus::InternalError,
            ChainExchangeResponseStatus::BadRequest,
            ChainExchangeResponseStatus::Other(1),
        ])
        .unwrap()
    }
}

impl Serialize for ChainExchangeResponseStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use ChainExchangeResponseStatus::*;
        let code: i32 = match self {
            Success => 0,
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

impl<'de> Deserialize<'de> for ChainExchangeResponseStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        let code: i32 = Deserialize::deserialize(deserializer)?;

        use ChainExchangeResponseStatus::*;
        let status = match code {
            0 => Success,
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

/// The response to a `ChainExchange` request.
#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct ChainExchangeResponse {
    /// Status code of the response.
    pub status: ChainExchangeResponseStatus,
    /// Status message indicating failure reason.
    pub message: String,
    /// The tipsets requested.
    pub chain: Vec<TipsetBundle>,
}

impl ChainExchangeResponse {
    /// Converts `chain_exchange` response into result.
    /// Returns an error if the response status is not `Ok`.
    /// Tipset bundle is converted into generic return type with `TryFrom` trait
    /// implementation.
    pub fn into_result<T>(self) -> Result<Vec<T>, String>
    where
        T: TryFrom<TipsetBundle, Error = String>,
    {
        if self.status != ChainExchangeResponseStatus::Success
            && self.status != ChainExchangeResponseStatus::PartialResponse
        {
            return Err(format!("Status {:?}: {}", self.status, self.message));
        }

        self.chain.into_iter().map(T::try_from).collect()
    }
}
/// Contains all BLS and SECP messages and their indexes per block
#[derive(Clone, Debug, PartialEq, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct CompactedMessages {
    /// Unsigned BLS messages.
    pub bls_msgs: Vec<Message>,
    /// Describes which block each message belongs to.
    /// if `bls_msg_includes[2] = vec![5]` then `TipsetBundle.blocks[2]` contains `bls_msgs[5]`
    pub bls_msg_includes: Vec<Vec<u64>>,

    /// Signed SECP messages.
    pub secp_msgs: Vec<SignedMessage>,
    /// Describes which block each message belongs to.
    pub secp_msg_includes: Vec<Vec<u64>>,
}

/// Contains the blocks and messages in a particular tipset
#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple, Default)]
pub struct TipsetBundle {
    /// The blocks in the tipset.
    pub blocks: Vec<BlockHeader>,

    /// Compressed messages format.
    pub messages: Option<CompactedMessages>,
}

impl TryFrom<TipsetBundle> for Tipset {
    type Error = String;

    fn try_from(tsb: TipsetBundle) -> Result<Self, Self::Error> {
        Tipset::new(tsb.blocks).map_err(|e| e.to_string())
    }
}

impl TryFrom<TipsetBundle> for Arc<Tipset> {
    type Error = String;

    fn try_from(tsb: TipsetBundle) -> Result<Self, Self::Error> {
        Tipset::try_from(tsb).map(Arc::new)
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

/// Constructs a [`FullTipset`] from headers and compacted messages from a
/// bundle.
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
                    "Block {i} in bundle has too many messages ({message_count} > {BLOCK_MESSAGE_LIMIT})"
                ));
            }
            let bls_messages = values_from_indexes(&bls_msg_includes[i], bls_msgs)?;
            let secp_messages = values_from_indexes(&secp_msg_includes[i], secp_msgs)?;

            Ok(Block {
                header,
                bls_messages,
                secp_messages,
            })
        })
        .collect::<Result<_, _>>()?;

    FullTipset::new(blocks).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use quickcheck_macros::quickcheck;
    use serde_json;

    use super::*;

    #[quickcheck]
    fn chain_exchange_response_status_roundtrip(status: ChainExchangeResponseStatus) {
        let serialized = serde_json::to_string(&status).unwrap();
        let parsed = serde_json::from_str(&serialized).unwrap();
        assert_eq!(status, parsed);
    }
}
