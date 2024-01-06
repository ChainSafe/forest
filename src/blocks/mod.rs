// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use thiserror::Error;

mod block;
mod election_proof;
mod gossip_block;
mod header;
mod ticket;
#[cfg(not(doc))]
mod tipset;
#[cfg(doc)]
pub mod tipset;
mod vrf_proof;

pub use block::{Block, TxMeta, BLOCK_MESSAGE_LIMIT};
pub use election_proof::ElectionProof;
pub use gossip_block::GossipBlock;
pub use header::{CachingBlockHeader, RawBlockHeader};
pub use ticket::Ticket;
pub use tipset::{FullTipset, Tipset, TipsetKeys};
pub use vrf_proof::VRFProof;

/// Blockchain blocks error
#[derive(Debug, PartialEq, Eq, Error)]
pub enum Error {
    /// Tipset contains invalid data, as described by the string parameter.
    #[error("Invalid tipset: {0}")]
    InvalidTipset(String),
    /// The given tipset has no blocks
    #[error("No blocks for tipset")]
    NoBlocks,
    /// Invalid signature
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
    /// Error in validating arbitrary data
    #[error("Error validating data: {0}")]
    Validation(String),
}

#[cfg(test)]
mod tests {
    mod serialization_vectors;
    mod ticket_test;
}
