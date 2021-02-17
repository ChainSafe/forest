// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use encoding::{Byte32De, BytesSer};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// String of random bytes usually generated from a randomness beacon or from tickets on chain.
#[derive(PartialEq, Eq, Default, Copy, Clone, Debug)]
pub struct Randomness(pub [u8; 32]);

impl Serialize for Randomness {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        BytesSer(&self.0).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Randomness {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = Byte32De::deserialize(deserializer)?;
        Ok(Self(bytes.0))
    }
}
