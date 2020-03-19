// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use encoding::Cbor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// State is reponsible for creating
pub struct State {}

impl State {
    pub fn new() -> Self {
        Self {}
    }
}

impl Serialize for State {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let empty: [u8; 0] = [];
        empty.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for State {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let _: [u8; 0] = Deserialize::deserialize(deserializer)?;

        Ok(Self {})
    }
}

impl Cbor for State {}
