// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::convert::TryFrom;

use crate::blocks::{BLOCK_MESSAGE_LIMIT, Block, CachingBlockHeader, FullTipset, Tipset};
use crate::message::SignedMessage;
use crate::shim::message::Message;
use crate::shim::policy::policy_constants::CHAIN_FINALITY;
use anyhow::Context as _;
use cid::Cid;
use fvm_ipld_encoding::tuple::*;
use nunny::Vec as NonEmpty;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// `ChainExchange` Filecoin header set bit.
pub const HEADERS: u64 = 0b01;
/// `ChainExchange` Filecoin messages set bit.
pub const MESSAGES: u64 = 0b10;

/// The payload that gets sent to another node to request for blocks and
/// messages.
#[derive(Clone, Debug, PartialEq, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct ChainExchangeRequest {
    /// The tipset [Cid] to start the request from.
    pub start: NonEmpty<Cid>,
    /// The amount of tipsets to request.
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

    /// If either the [HEADERS] bit or the [MESSAGES] bit is set.
    pub fn is_options_valid(&self) -> bool {
        self.include_blocks() || self.include_messages()
    }

    /// Checks if the request length is within `(0, CHAIN_FINALITY]`, matching
    /// Lotus's [`MaxRequestLength`].
    ///
    /// [`MaxRequestLength`]: https://github.com/filecoin-project/lotus/blob/v1.35.1/chain/exchange/protocol.go#L30
    pub fn is_request_len_valid(&self) -> bool {
        self.request_len > 0 && self.request_len <= CHAIN_FINALITY as u64
    }
}

/// Status codes of a `chain_exchange` response.
#[derive(Clone, Debug, PartialEq, Eq, Copy)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
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
    Other(#[cfg_attr(test, arbitrary(gen(|_| 1)))] i32),
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
    /// Build a [`ChainExchangeResponseStatus::GoAway`] response asking the
    /// requester to back off (e.g. when concurrent-request caps are reached).
    pub fn go_away(message: impl Into<String>) -> Self {
        Self {
            chain: Default::default(),
            status: ChainExchangeResponseStatus::GoAway,
            message: message.into(),
        }
    }

    /// Converts `chain_exchange` response into result.
    /// Returns an error if the response status is not `Ok`.
    /// Tipset bundle is converted into generic return type with `TryFrom` trait
    /// implementation.
    pub fn into_result<T>(self) -> anyhow::Result<Vec<T>>
    where
        T: TryFrom<TipsetBundle>,
        <T as TryFrom<TipsetBundle>>::Error: Into<anyhow::Error>,
    {
        if self.status != ChainExchangeResponseStatus::Success
            && self.status != ChainExchangeResponseStatus::PartialResponse
        {
            anyhow::bail!("Status {:?}: {}", self.status, self.message);
        }

        self.chain
            .into_iter()
            .map(|i| {
                T::try_from(i).map_err(|e| e.into().context("failed to convert from tipset bundle"))
            })
            .collect()
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
    pub blocks: Vec<CachingBlockHeader>,

    /// Compressed messages format.
    pub messages: Option<CompactedMessages>,
}

impl TryFrom<TipsetBundle> for Tipset {
    type Error = anyhow::Error;

    fn try_from(tsb: TipsetBundle) -> Result<Self, Self::Error> {
        Ok(Tipset::new(tsb.blocks)?)
    }
}

impl TryFrom<TipsetBundle> for CompactedMessages {
    type Error = anyhow::Error;

    fn try_from(tsb: TipsetBundle) -> Result<Self, Self::Error> {
        tsb.messages.context("Request contained no messages")
    }
}

impl TryFrom<TipsetBundle> for FullTipset {
    type Error = anyhow::Error;

    fn try_from(tsb: TipsetBundle) -> Result<FullTipset, Self::Error> {
        fts_from_bundle_parts(tsb.blocks, tsb.messages.as_ref())
    }
}

impl TryFrom<&TipsetBundle> for FullTipset {
    type Error = anyhow::Error;

    fn try_from(tsb: &TipsetBundle) -> Result<FullTipset, Self::Error> {
        fts_from_bundle_parts(tsb.blocks.clone(), tsb.messages.as_ref())
    }
}

/// Constructs a [`FullTipset`] from headers and compacted messages from a
/// bundle.
fn fts_from_bundle_parts(
    headers: Vec<CachingBlockHeader>,
    messages: Option<&CompactedMessages>,
) -> anyhow::Result<FullTipset> {
    let CompactedMessages {
        bls_msgs,
        bls_msg_includes,
        secp_msg_includes,
        secp_msgs,
    } = messages.context("Tipset bundle did not contain message bundle")?;

    if headers.len() != bls_msg_includes.len() || headers.len() != secp_msg_includes.len() {
        anyhow::bail!(
            "Invalid formed Tipset bundle, lengths of includes does not match blocks. Header len: {}, bls_msg len: {}, secp_msg len: {}",
            headers.len(),
            bls_msg_includes.len(),
            secp_msg_includes.len()
        );
    }
    let zipped = headers
        .into_iter()
        .zip(bls_msg_includes.iter())
        .zip(secp_msg_includes.iter());

    fn values_from_indexes<T: Clone>(indexes: &[u64], values: &[T]) -> anyhow::Result<Vec<T>> {
        indexes
            .iter()
            .map(|idx| {
                values
                    .get(*idx as usize)
                    .cloned()
                    .context("Invalid message index")
            })
            .collect()
    }

    let blocks = zipped
        .enumerate()
        .map(|(i, ((header, bls_msg_include), secp_msg_include))| {
            let message_count = bls_msg_include.len() + secp_msg_include.len();
            if message_count > BLOCK_MESSAGE_LIMIT {
                anyhow::bail!(
                    "Block {i} in bundle has too many messages ({message_count} > {BLOCK_MESSAGE_LIMIT})"
                );
            }
            let bls_messages = values_from_indexes(bls_msg_include, bls_msgs)?;
            let secp_messages = values_from_indexes(secp_msg_include, secp_msgs)?;

            Ok(Block {
                header,
                bls_messages,
                secp_messages,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(FullTipset::new(blocks)?)
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
