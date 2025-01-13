// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::bigint::BigInt;
use crate::shim::clock::ChainEpoch;
use cid::Cid;
use nunny::Vec as NonEmpty;
use serde_tuple::{self, Deserialize_tuple, Serialize_tuple};

/// Hello message <https://filecoin-project.github.io/specs/#hello-spec>
#[derive(Clone, Debug, PartialEq, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct HelloRequest {
    pub heaviest_tip_set: NonEmpty<Cid>,
    pub heaviest_tipset_height: ChainEpoch,
    pub heaviest_tipset_weight: BigInt,
    pub genesis_cid: Cid,
}

/// Response to a Hello message. This just handles latency of the peer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct HelloResponse {
    /// Time of arrival to peer in UNIX nanoseconds.
    pub arrival: u64,
    /// Time sent from peer in UNIX nanoseconds.
    pub sent: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::encoding::from_slice_with_fallback;
    use crate::utils::multihash::prelude::*;
    use fvm_ipld_encoding::{to_vec, DAG_CBOR};

    #[test]
    fn hello_default_ser() {
        let orig_msg = HelloRequest {
            genesis_cid: Cid::new_v1(DAG_CBOR, MultihashCode::Identity.digest(&[])),
            heaviest_tipset_weight: Default::default(),
            heaviest_tipset_height: Default::default(),
            heaviest_tip_set: NonEmpty::of(Default::default()),
        };
        let bz = to_vec(&orig_msg).unwrap();
        let msg: HelloRequest = from_slice_with_fallback(&bz).unwrap();
        assert_eq!(msg, orig_msg);
    }
}
