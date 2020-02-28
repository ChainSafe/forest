// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_cid::Cid;
use forest_encoding::{
    de::{Deserialize, Deserializer},
    ser::{Serialize, Serializer},
};

/// Hello message https://filecoin-project.github.io/specs/#hello-spec
#[derive(Clone, Debug, PartialEq, Default)]
pub struct HelloMessage {
    heaviest_tip_set: Vec<Cid>,
    heaviest_tipset_weight: u64,
    heaviest_tipset_height: u64,
    genesis_hash: Cid,
}

impl Serialize for HelloMessage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.heaviest_tip_set,
            &self.heaviest_tipset_weight,
            &self.heaviest_tipset_height,
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
        let (heaviest_tip_set, heaviest_tipset_weight, heaviest_tipset_height, genesis_hash) =
            Deserialize::deserialize(deserializer)?;
        Ok(HelloMessage {
            heaviest_tip_set,
            heaviest_tipset_weight,
            heaviest_tipset_height,
            genesis_hash,
        })
    }
}
/// Response to a Hello
#[derive(Clone, Debug, PartialEq)]
pub struct LatencyMessage {
    /// Time of arrival in unix nanoseconds
    arrival: u64,
    /// Time sent in unix nanoseconds
    sent: u64,
}
impl Serialize for LatencyMessage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.arrival, &self.sent).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for LatencyMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (arrival, sent) = Deserialize::deserialize(deserializer)?;
        Ok(LatencyMessage { arrival, sent })
    }
}
