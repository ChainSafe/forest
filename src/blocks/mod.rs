// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use thiserror::Error;

mod block;
#[cfg(test)]
mod chain4u;
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
pub use tipset::{CreateTipsetError, FullTipset, Tipset, TipsetKey};
pub use vrf_proof::VRFProof;

/// Blockchain blocks error
#[derive(Debug, PartialEq, Eq, Error)]
pub enum Error {
    /// Invalid signature
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
    /// Error in validating arbitrary data
    #[error("Error validating data: {0}")]
    Validation(String),
}

#[cfg(test)]
pub(crate) use chain4u::{chain4u, Chain4U, HeaderBuilder};

#[cfg(any(test, doc))]
mod tests {

    mod serialization_vectors;
    mod ticket_test;
}
