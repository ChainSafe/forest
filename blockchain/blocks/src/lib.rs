// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod block;
pub mod election_proof;
mod errors;
pub mod gossip_block;
pub mod header;
pub mod ticket;
pub mod tipset;

pub use block::*;
use cid::Cid;
pub use election_proof::*;
pub use errors::*;
pub use gossip_block::*;
pub use header::*;
pub use ticket::*;
pub use tipset::*;

#[derive(Clone)]
struct ArbitraryCid(Cid);

impl quickcheck::Arbitrary for ArbitraryCid {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        ArbitraryCid(Cid::new_v1(
            u64::arbitrary(g),
            cid::multihash::Multihash::wrap(u64::arbitrary(g), &[u8::arbitrary(g)]).unwrap(),
        ))
    }
}
