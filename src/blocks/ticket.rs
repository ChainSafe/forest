// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::VRFProof;
use serde_tuple::{self, Deserialize_tuple, Serialize_tuple};

/// A Ticket is a marker of a tick of the blockchain's clock.  It is the source
/// of randomness for proofs of storage and leader election.  It is generated
/// by the miner of a block using a `VRF` and a `VDF`.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize_tuple, Deserialize_tuple)]
pub struct Ticket {
    /// A proof output by running a `VRF` on the `VDFResult` of the parent
    /// ticket
    pub vrfproof: VRFProof,
}

impl Ticket {
    /// Ticket constructor
    pub fn new(vrfproof: VRFProof) -> Self {
        Self { vrfproof }
    }
}

#[cfg(test)]
impl quickcheck::Arbitrary for Ticket {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let fmt_str = format!("===={}=====", u64::arbitrary(g));
        let vrfproof = VRFProof::new(fmt_str.into_bytes());
        Self { vrfproof }
    }
}
