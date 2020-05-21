// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use encoding::{Byte32De, BytesSer};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{self, Debug, Formatter};

/// String of random bytes
#[derive(PartialEq, Eq, Default, Copy, Clone)]
pub struct Randomness(pub [u8; 32]);

impl Debug for Randomness {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

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
