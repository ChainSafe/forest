// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod errors;
mod network;
mod payload;
mod protocol;
pub use self::errors::Error;
pub use self::network::Network;
pub use self::payload::{BLSPublicKey, Payload};
pub use self::protocol::Protocol;

#[allow(unused_imports)]
use data_encoding_macro::{internal_new_encoding, new_encoding};
use encoding::blake2b_variable;
use once_cell::sync::OnceCell;

pub use fvm_shared::address::Address;

/// Hash length of payload for Secp and Actor addresses.
pub const PAYLOAD_HASH_LEN: usize = 20;

/// Uncompressed secp public key used for validation of Secp addresses.
pub const SECP_PUB_LEN: usize = 65;

/// BLS public key length used for validation of BLS addresses.
pub const BLS_PUB_LEN: usize = 48;

/// Length of the checksum hash for string encodings.
pub const CHECKSUM_HASH_LEN: usize = 4;

#[cfg(feature = "json")]
const UNDEF_ADDR_STRING: &str = "<empty>";

// TODO pull network from config (probably)
pub static NETWORK_DEFAULT: OnceCell<Network> = OnceCell::new();

#[cfg(test)]
mod tests {
    // Test cases for FOR-02: https://github.com/ChainSafe/forest/issues/1134
    // use crate::{errors::Error, from_leb_bytes, to_leb_bytes};

    // FIXME: Is this tested in the fvm crate?
    // #[test]
    // fn test_from_leb_bytes_passing() {
    //     let passing = vec![67];
    //     assert_eq!(
    //         to_leb_bytes(from_leb_bytes(&passing).unwrap()),
    //         Ok(vec![67])
    //     );
    // }

    // FIXME: Is this tested in the fvm crate?
    // #[test]
    // fn test_from_leb_bytes_extra_bytes() {
    //     let extra_bytes = vec![67, 0, 1, 2];

    //     match from_leb_bytes(&extra_bytes) {
    //         Ok(id) => {
    //             println!(
    //                 "Successfully decoded bytes when it was not supposed to. Result was: {:?}",
    //                 &to_leb_bytes(id).unwrap()
    //             );
    //             panic!();
    //         }
    //         Err(e) => {
    //             assert_eq!(e, Error::InvalidAddressIDPayload(extra_bytes));
    //         }
    //     }
    // }

    // FIXME: Is this tested in the fvm crate?
    // #[test]
    // fn test_from_leb_bytes_minimal_encoding() {
    //     let minimal_encoding = vec![67, 0, 130, 0];

    //     match from_leb_bytes(&minimal_encoding) {
    //         Ok(id) => {
    //             println!(
    //                 "Successfully decoded bytes when it was not supposed to. Result was: {:?}",
    //                 &to_leb_bytes(id).unwrap()
    //             );
    //             panic!();
    //         }
    //         Err(e) => {
    //             assert_eq!(e, Error::InvalidAddressIDPayload(minimal_encoding));
    //         }
    //     }
    // }
}

/// Checksum calculates the 4 byte checksum hash
pub fn checksum(ingest: &[u8]) -> Vec<u8> {
    blake2b_variable(ingest, CHECKSUM_HASH_LEN)
}

/// Validates the checksum against the ingest data
pub fn validate_checksum(ingest: &[u8], expect: Vec<u8>) -> bool {
    let digest = checksum(ingest);
    digest == expect
}

#[cfg(feature = "json")]
pub mod json {
    use super::*;
    use serde::{Deserialize, Deserializer, Serializer};
    use std::borrow::Cow;

    /// Wrapper for serializing and deserializing a SignedMessage from JSON.
    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct AddressJson(#[serde(with = "self")] pub Address);

    /// Wrapper for serializing a SignedMessage reference to JSON.
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
        serializer.serialize_str(&m.encode())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Address, D::Error>
    where
        D: Deserializer<'de>,
    {
        let address_as_string: Cow<'de, str> = Deserialize::deserialize(deserializer)?;
        Address::from_str(&address_as_string).map_err(de::Error::custom)
    }

    #[cfg(feature = "json")]
    pub mod vec {
        use super::*;
        use crate::json::{AddressJson, AddressJsonRef};
        use forest_json_utils::GoVecVisitor;
        use serde::ser::SerializeSeq;

        /// Wrapper for serializing and deserializing a Cid vector from JSON.
        #[derive(Deserialize, Serialize)]
        #[serde(transparent)]
        pub struct AddressJsonVec(#[serde(with = "self")] pub Vec<Address>);

        /// Wrapper for serializing a cid slice to JSON.
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

        pub fn serialize<S>(v: &Option<Address>, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            if let Some(unwrapped_address) = v.as_ref() {
                serializer.serialize_str(&unwrapped_address.encode())
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
