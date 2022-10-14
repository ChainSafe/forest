// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared::address::Address;

pub mod json {
    use super::*;
    use serde::{de, Serialize};
    use serde::{Deserialize, Deserializer, Serializer};
    use std::borrow::Cow;
    use std::str::FromStr;

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
        use super::*;
        use super::{AddressJson, AddressJsonRef};
        use forest_utils::json::GoVecVisitor;
        use serde::ser::SerializeSeq;

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
        use super::*;
        use serde::{self, Deserialize, Deserializer, Serializer};
        use std::borrow::Cow;

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
    use super::*;
    use quickcheck_macros::quickcheck;
    use serde_json;

    #[cfg(test)]
    #[derive(Clone, Debug, PartialEq)]
    struct AddressWrapper {
        address: Address,
    }

    #[cfg(test)]
    impl quickcheck::Arbitrary for AddressWrapper {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let address = arbitrary::Arbitrary::arbitrary(&mut arbitrary::Unstructured::new(
                &Vec::arbitrary(g),
            ))
            .unwrap();
            AddressWrapper { address }
        }
    }

    #[quickcheck]
    fn address_roundtrip(address: AddressWrapper) {
        let serialized = serde_json::to_string(&json::AddressJsonRef(&address.address)).unwrap();
        let parsed: json::AddressJson = serde_json::from_str(&serialized).unwrap();
        assert_eq!(address.address, parsed.0);
    }
}
