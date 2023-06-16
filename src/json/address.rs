// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_shim::address::Address;

pub mod json {
    use std::{borrow::Cow, str::FromStr};

    use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

    use super::*;

    /// Wrapper for serializing and de-serializing a `SignedMessage` from JSON.
    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct AddressJson(#[serde(with = "self")] pub Address);

    /// Wrapper for serializing a `SignedMessage` reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct AddressJsonRef<'a>(#[serde(with = "self")] pub &'a Address);

    impl From<Address> for AddressJson {
        fn from(address: Address) -> Self {
            Self(address)
        }
    }

    impl From<AddressJson> for Address {
        fn from(address: AddressJson) -> Self {
            address.0
        }
    }

    pub fn serialize<S>(m: &Address, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&m.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Address, D::Error>
    where
        D: Deserializer<'de>,
    {
        let address_as_string: Cow<'de, str> = Deserialize::deserialize(deserializer)?;
        Address::from_str(&address_as_string).map_err(de::Error::custom)
    }

    pub mod vec {
        use forest_utils::json::GoVecVisitor;
        use serde::ser::SerializeSeq;

        use super::{AddressJson, AddressJsonRef, *};

        /// Wrapper for serializing and de-serializing a Cid vector from JSON.
        #[derive(Deserialize, Serialize)]
        #[serde(transparent)]
        pub struct AddressJsonVec(#[serde(with = "self")] pub Vec<Address>);

        /// Wrapper for serializing a CID slice to JSON.
        #[derive(Serialize)]
        #[serde(transparent)]
        pub struct AddressJsonSlice<'a>(#[serde(with = "self")] pub &'a [Address]);

        pub fn serialize<S>(m: &[Address], serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut seq = serializer.serialize_seq(Some(m.len()))?;
            for e in m {
                seq.serialize_element(&AddressJsonRef(e))?;
            }
            seq.end()
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Address>, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(GoVecVisitor::<Address, AddressJson>::new())
        }
    }

    pub mod opt {
        use std::borrow::Cow;

        use serde::{self, Deserialize, Deserializer, Serializer};

        use super::*;

        const UNDEF_ADDR_STRING: &str = "<empty>";

        pub fn serialize<S>(v: &Option<Address>, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            if let Some(unwrapped_address) = v.as_ref() {
                serializer.serialize_str(&unwrapped_address.to_string())
            } else {
                serializer.serialize_str(UNDEF_ADDR_STRING)
            }
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Address>, D::Error>
        where
            D: Deserializer<'de>,
        {
            let address_as_string: Cow<'de, str> = Deserialize::deserialize(deserializer)?;
            if address_as_string == UNDEF_ADDR_STRING {
                return Ok(None);
            }
            Ok(Some(
                Address::from_str(&address_as_string).map_err(de::Error::custom)?,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use quickcheck_macros::quickcheck;
    use serde_json;

    use super::*;

    #[quickcheck]
    fn address_roundtrip(address: Address) {
        let serialized = forest_test_utils::to_string_with!(&address, json::serialize);
        let parsed = forest_test_utils::from_str_with!(&serialized, json::deserialize);
        // Skip delegated addresses for now
        if address.protocol() != forest_shim::address::Protocol::Delegated {
            assert_eq!(address, parsed)
        }
    }
}
