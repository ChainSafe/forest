// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod block;
pub mod election_proof;
mod errors;
pub mod gossip_block;
pub mod header;
pub mod persistence;
pub mod ticket;
pub mod tipset;
mod vrf_proof;

pub use block::*;
pub use election_proof::ElectionProof;
pub use errors::*;
pub use gossip_block::GossipBlock;
pub use header::BlockHeader;
pub use ticket::Ticket;
pub use tipset::*;
pub use vrf_proof::VRFProof;

#[cfg(test)]
mod tests {
    mod header_json_test;
    mod serialization_vectors;
    mod ticket_test;
}
