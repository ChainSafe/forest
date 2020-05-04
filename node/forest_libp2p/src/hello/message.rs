// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use clock::ChainEpoch;
use forest_cid::Cid;
use forest_encoding::{de::Deserializer, ser::Serializer};
use num_bigint::{
    biguint_ser::{BigUintDe, BigUintSer},
    BigUint,
};
use serde::{Deserialize, Serialize};

/// Hello message https://filecoin-project.github.io/specs/#hello-spec
#[derive(Clone, Debug, PartialEq, Default)]
pub struct HelloMessage {
    pub heaviest_tip_set: Vec<Cid>,
    pub heaviest_tipset_height: ChainEpoch,
    pub heaviest_tipset_weight: BigUint,
    pub genesis_hash: Cid,
}

impl Serialize for HelloMessage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.heaviest_tip_set,
            &self.heaviest_tipset_height,
            BigUintSer(&self.heaviest_tipset_weight),
            &self.genesis_hash,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for HelloMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (
            heaviest_tip_set,
            heaviest_tipset_height,
            BigUintDe(heaviest_tipset_weight),
            genesis_hash,
        ) = Deserialize::deserialize(deserializer)?;

        Ok(HelloMessage {
            heaviest_tip_set,
            heaviest_tipset_height,
            heaviest_tipset_weight,
            genesis_hash,
        })
    }
}

/// Response to a Hello
#[derive(Clone, Debug, PartialEq)]
pub struct HelloResponse {
    /// Time of arrival in unix nanoseconds
    pub arrival: i64,
    /// Time sent in unix nanoseconds
    pub sent: i64,
}

impl Serialize for HelloResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.arrival, &self.sent).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for HelloResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (arrival, sent) = Deserialize::deserialize(deserializer)?;
        Ok(HelloResponse { arrival, sent })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forest_cid::multihash::Identity;
    use forest_encoding::*;

    #[test]
    fn hello_default_ser() {
        let orig_msg = HelloMessage {
            genesis_hash: Cid::new_from_cbor(&[], Identity),
            ..Default::default()
        };
        let bz = to_vec(&orig_msg).unwrap();
        let msg: HelloMessage = from_slice(&bz).unwrap();
        assert_eq!(msg, orig_msg);
    }
}
