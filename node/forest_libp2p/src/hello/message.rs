// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use clock::ChainEpoch;
use forest_cid::Cid;
use forest_encoding::tuple::*;
use num_bigint::BigUint;

/// Hello message https://filecoin-project.github.io/specs/#hello-spec
#[derive(Clone, Debug, PartialEq, Default, Serialize_tuple, Deserialize_tuple)]
pub struct HelloRequest {
    pub heaviest_tip_set: Vec<Cid>,
    pub heaviest_tipset_height: ChainEpoch,
    #[serde(with = "num_bigint::biguint_ser")]
    pub heaviest_tipset_weight: BigUint,
    pub genesis_hash: Cid,
}

/// Response to a Hello
#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct HelloResponse {
    /// Time of arrival in unix nanoseconds
    pub arrival: i64,
    /// Time sent in unix nanoseconds
    pub sent: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use forest_cid::multihash::Identity;
    use forest_encoding::*;

    #[test]
    fn hello_default_ser() {
        let orig_msg = HelloRequest {
            genesis_hash: Cid::new_from_cbor(&[], Identity),
            ..Default::default()
        };
        let bz = to_vec(&orig_msg).unwrap();
        let msg: HelloRequest = from_slice(&bz).unwrap();
        assert_eq!(msg, orig_msg);
    }
}
