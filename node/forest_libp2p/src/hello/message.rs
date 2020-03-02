// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use clock::ChainEpoch;
use forest_cid::Cid;
use forest_encoding::{de::Deserializer, ser::Serializer};
use num_bigint::{bigint_ser, BigInt};
use serde::{Deserialize, Serialize};

/// Hello message https://filecoin-project.github.io/specs/#hello-spec
#[derive(Clone, Debug, PartialEq, Default)]
pub struct HelloMessage {
    pub heaviest_tip_set: Vec<Cid>,
    pub heaviest_tipset_height: ChainEpoch,
    pub heaviest_tipset_weight: BigInt,
    pub genesis_hash: Cid,
}

impl Serialize for HelloMessage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct TupleHelloMessage<'a>(
            &'a [Cid],
            &'a ChainEpoch,
            #[serde(with = "bigint_ser")] &'a BigInt,
            &'a Cid,
        );
        TupleHelloMessage(
            &self.heaviest_tip_set,
            &self.heaviest_tipset_height,
            &self.heaviest_tipset_weight,
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
        #[derive(Deserialize)]
        struct TupleHelloMessage(
            Vec<Cid>,
            ChainEpoch,
            #[serde(with = "bigint_ser")] BigInt,
            Cid,
        );
        let TupleHelloMessage(
            heaviest_tip_set,
            heaviest_tipset_height,
            heaviest_tipset_weight,
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
