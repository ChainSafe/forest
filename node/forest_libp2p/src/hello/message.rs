// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use forest_encoding::tuple::*;
use fvm_shared::bigint::BigInt;
use fvm_shared::clock::ChainEpoch;

/// Hello message <https://filecoin-project.github.io/specs/#hello-spec>
#[derive(Clone, Debug, PartialEq, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct HelloRequest {
    pub heaviest_tip_set: Vec<Cid>,
    pub heaviest_tipset_height: ChainEpoch,
    #[serde(with = "fvm_shared::bigint::bigint_ser")]
    pub heaviest_tipset_weight: BigInt,
    pub genesis_hash: Cid,
}

/// Response to a Hello message. This just handles latency of the peer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize_tuple, Deserialize_tuple)]
pub struct HelloResponse {
    /// Time of arrival to peer in unix nanoseconds.
    pub arrival: u64,
    /// Time sent from peer in unix nanoseconds.
    pub sent: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use cid::multihash::Code::Identity;
    use cid::multihash::MultihashDigest;
    use forest_encoding::*;
    use fvm_ipld_encoding::DAG_CBOR;

    #[test]
    fn hello_default_ser() {
        let orig_msg = HelloRequest {
            genesis_hash: Cid::new_v1(DAG_CBOR, Identity.digest(&[])),
            heaviest_tipset_weight: Default::default(),
            heaviest_tipset_height: Default::default(),
            heaviest_tip_set: Default::default(),
        };
        let bz = to_vec(&orig_msg).unwrap();
        let msg: HelloRequest = from_slice(&bz).unwrap();
        assert_eq!(msg, orig_msg);
    }
}
