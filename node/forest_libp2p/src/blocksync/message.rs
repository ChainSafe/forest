// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_blocks::{Block, BlockHeader, FullTipset, Tipset, BLOCK_MESSAGE_LIMIT};
use forest_encoding::tuple::*;
use forest_message::{SignedMessage, UnsignedMessage};
use std::convert::TryFrom;

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
