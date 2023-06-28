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

pub use block::*;
pub use election_proof::ElectionProof;
pub use errors::*;
pub use gossip_block::GossipBlock;
pub use header::BlockHeader;
pub use ticket::Ticket;
pub use tipset::*;

#[cfg(test)]
#[derive(Clone)]
struct ArbitraryCid(cid::Cid);

#[cfg(test)]
impl quickcheck::Arbitrary for ArbitraryCid {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        ArbitraryCid(cid::Cid::new_v1(
            u64::arbitrary(g),
            cid::multihash::Multihash::wrap(u64::arbitrary(g), &[u8::arbitrary(g)]).unwrap(),
        ))
    }
}
#[cfg(test)]
mod tests {
    mod header_json_test;
    mod serialization_vectors;
    mod ticket_test;
}
