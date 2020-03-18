// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use encoding::Cbor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// State includes the address for the actor
pub struct State {
    pub address: Address,
}

impl Serialize for State {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        [&self.address].serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for State {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let [address]: [Address; 1] = Deserialize::deserialize(deserializer)?;
        Ok(Self { address })
    }
}

impl Cbor for State {}
