// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! In the filecoin ecosystem, there are TWO different ways to present a domain object:
//! - CBOR (defined in [fvm_ipld_encoding]).
//!   This is the wire format.
//! - JSON (see [serde_json]).
//!   This is used in e.g RPC code, or in lotus printouts
//!
//! We care about compatibility with lotus/the filecoin ecosystem for both.
//! This module explores how we lay that out in code.
//!
//! # Terminology
//! - A "domain object" is a concept of an object.
//!   E.g "a CID with version = 1, codec = 0, and a multihash which is all zero"
//!   (This happens to be the default CID).
//! - The "in memory" representation is how (rust) lays that out in memory.
//!   See the definition of [`struct Cid { .. }`](`::cid::Cid`).
//! - The "lotus json" is how [lotus](https://github.com/filecoin-project/lotus),
//!   the reference filecoin implementation, displays that object in json.
//!   ```json
//!   { "/": "baeaaaaa" }
//!   ```
//! - The "lotus cbor" is how lotus represents that object on the wire.
//!   ```rust
//!   let in_memory = ::cid::Cid::default();
//!   let cbor = fvm_ipld_encoding::to_vec(&in_memory).unwrap();
//!   assert_eq!(
//!       cbor,
//!       0b_11011000_00101010_01000101_00000000_00000001_00000000_00000000_00000000_u64.to_be_bytes(),
//!   );
//!   ```
//!
//! In rust, the most common serialization framework is [serde].
//! It has ONE (de)serialization model for each struct.
//!
//! The way forest currently does this is
//!
//!
//!

//! Lotus' json presentation has some quirks.
//! These are helpers for (de)serialising data that has to match those.
//!
//! **Note: writing custom deserialisation code is considered harmful.**
//! If you are about to write a `mod json` or a `mod vec`, please think twice!

use serde::{ser::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use std::{convert::Infallible, fmt::Display, str::FromStr as _};

pub trait LotusSerialize {
    fn serialize_cbor<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer;

    fn serialize_json<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer;
}

pub trait LotusDeserialize<'de>: Sized {
    fn deserialize_cbor<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>;

    fn deserialize_json<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>;
}

impl<T> LotusSerialize for T
where
    T: Serialize, // CBOR
    T: HasLotusJson,
    T::LotusJson: Serialize,
    for<'a> &'a T: TryInto<T::LotusJson>,
    for<'a> <&'a T as TryInto<T::LotusJson>>::Error: Display,
{
    fn serialize_cbor<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.serialize(serializer)
    }

    fn serialize_json<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.try_into() {
            Ok(o) => o.serialize(serializer),
            Err(e) => Err(S::Error::custom(e)),
        }
    }
}

struct Foo {
    cid: ::cid::Cid,
}

struct FooJson {
    cid: <::cid::Cid as HasLotusJson>::LotusJson,
}

// TODO(aatifsyed): #[derive(HasLotusJson)]
pub trait HasLotusJson {
    type LotusJson;
}

mod cid2 {
    use super::*;

    impl<const S: usize> HasLotusJson for ::cid::CidGeneric<S> {
        type LotusJson = CidLotusJson;
    }

    /// Structure just used as a helper to serialize a CID into a map with key "/"
    #[derive(Serialize, Deserialize)]
    pub struct CidLotusJson {
        #[serde(rename = "/")]
        cid: String,
    }

    impl<const S: usize> TryFrom<CidLotusJson> for ::cid::CidGeneric<S> {
        type Error = ::cid::Error;

        fn try_from(value: CidLotusJson) -> Result<Self, Self::Error> {
            Self::from_str(&value.cid)
        }
    }

    impl<const S: usize> TryFrom<::cid::CidGeneric<S>> for CidLotusJson {
        type Error = Infallible;

        fn try_from(value: ::cid::CidGeneric<S>) -> Result<Self, Self::Error> {
            Ok(Self {
                cid: value.to_string(),
            })
        }
    }
}

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
