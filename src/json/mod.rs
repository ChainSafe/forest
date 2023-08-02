// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Lotus' json presentation has some quirks.
//! These are helpers for (de)serialising data that has to match those.
//!
//! **Note: writing custom deserialisation code is considered harmful.**
//! If you are about to write a `mod json` or a `mod vec`, please think twice!

/// Lotus (de)serializes empty sequences as null.
/// This matches that behaviour.
// TODO(aatifsyed):
// - This is basically https://docs.rs/serde_with/latest/serde_with/struct.DefaultOnNull.html
// - Should we ask users to use `Option<Vec<T>>` or `Option<NonEmpty<Vec>>` instead?
pub mod empty_vec_is_null {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer, T: Serialize>(
        v: &Vec<T>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match v.is_empty() {
            true => serializer.serialize_none(),
            false => v.serialize(serializer),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
        deserializer: D,
    ) -> Result<Vec<T>, D::Error> {
        Option::<Vec<T>>::deserialize(deserializer).map(Option::unwrap_or_default)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        use serde_json::{from_value, json, to_value};

        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        #[serde(transparent)]
        struct SerdeMe(#[serde(with = "super")] Vec<usize>);

        #[test]
        fn empty() {
            assert_eq!(SerdeMe(Vec::new()), from_value(json!(null)).unwrap());
            assert_eq!(json!(null), to_value(SerdeMe(Vec::new())).unwrap());
        }

        #[test]
        fn one_element() {
            assert_eq!(SerdeMe(vec![1]), from_value(json!([1])).unwrap());
            assert_eq!(json!([1]), to_value(SerdeMe(vec![1])).unwrap());
        }
    }
}

/// Lotus (de)serializes empty sequences as null.
/// This matches that behaviour.
// TODO(aatifsyed): this shouldn't exist the `&[T]`s are all serialization helpers
// (we should just do the actual serialization on the parent struct)
pub mod empty_slice_is_null {
    use serde::{Serialize, Serializer};

    pub fn serialize<S: Serializer, T: Serialize>(
        v: &[T],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match v.is_empty() {
            true => serializer.serialize_none(),
            false => v.serialize(serializer),
        }
    }

    // pub fn deserialize<'de, D: Deserializer<'de>, T>(deserializer: D) -> Result<&'de [T], D::Error>
    // where
    //     &'de [T]: Deserialize<'de>,
    // {
    //     Option::<&'de [T]>::deserialize(deserializer).map(Option::unwrap_or_default)
    // }
}

pub mod actor_state;
pub mod address;
pub mod cid;
pub mod message;
pub mod message_receipt;
pub mod sector;
pub mod signature;
pub mod signed_message;
pub mod token_amount;
pub mod vrf;

#[cfg(test)]
mod tests {
    mod address_test;
    mod base_cid_tests;
    mod json_tests;
    mod serde_tests;
}
