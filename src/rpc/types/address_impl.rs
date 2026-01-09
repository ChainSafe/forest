// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::*;

const EMPTY_ADDRESS_VALUE: &str = "<empty>";

impl Serialize for AddressOrEmpty {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let address_bytes = match self.0 {
            Some(addr) => addr.to_string(),
            None => EMPTY_ADDRESS_VALUE.to_string(),
        };

        s.collect_str(&address_bytes)
    }
}

impl<'de> Deserialize<'de> for AddressOrEmpty {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let address_str = String::deserialize(deserializer)?;
        if address_str.eq(EMPTY_ADDRESS_VALUE) {
            return Ok(Self(None));
        }

        Address::from_str(&address_str)
            .map_err(de::Error::custom)
            .map(|addr| Self(Some(addr)))
    }
}
