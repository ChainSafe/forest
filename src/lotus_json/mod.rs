// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! In the Filecoin ecosystem, there are TWO different ways to present a domain object:
//! - CBOR (defined in [`fvm_ipld_encoding`]).
//!   This is the wire format.
//! - JSON (see [`serde_json`]).
//!   This is used in e.g RPC code, or in lotus printouts
//!
//! We care about compatibility with lotus/the Filecoin ecosystem for both.
//! This module defines traits and types for handling both.
//!
//! # Terminology and background
//! - A "domain object" is the _concept_ of an object.
//!   E.g `"a CID with version = 1, codec = 0, and a multihash which is all zero"`
//!   (This happens to be the default CID).
//! - The "in memory" representation is how (rust) lays that out in memory.
//!   See the definition of [`struct Cid { .. }`](`::cid::Cid`).
//! - The "lotus JSON" is how [lotus](https://github.com/filecoin-project/lotus),
//!   the reference Filecoin implementation, displays that object in JSON.
//!   ```json
//!   { "/": "baeaaaaa" }
//!   ```
//! - The "lotus CBOR" is how lotus represents that object on the wire.
//!   ```rust
//!   let in_memory = ::cid::Cid::default();
//!   let cbor = fvm_ipld_encoding::to_vec(&in_memory).unwrap();
//!   assert_eq!(
//!       cbor,
//!       0b_11011000_00101010_01000101_00000000_00000001_00000000_00000000_00000000_u64.to_be_bytes(),
//!   );
//!   ```
//!
//! In rust, the most common serialization framework is [`serde`].
//! It has ONE (de)serialization model for each struct - the serialization code _cannot_ know
//! if it's writing JSON or CBOR.
//!
//! The cleanest way handle the distinction would be a serde-compatible trait:
//! ```rust
//! # use serde::Serializer;
//! pub trait LotusSerialize {
//!     fn serialize_cbor<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//!     where
//!         S: Serializer;
//!
//!     fn serialize_json<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//!     where
//!         S: Serializer;
//! }
//! pub trait LotusDeserialize<'de> { /* ... */ }
//! ```
//!
//! However, that would require writing and maintaining a custom derive macro - can we lean on
//! [`macro@serde::Serialize`] and [`macro@serde::Deserialize`] instead?
//!
//! # Lotus JSON in Forest
//! - Have a struct which represents a domain object: e.g [`GossipBlock`](crate::blocks::GossipBlock).
//! - Implement [`serde::Serialize`] on that object, normally using [`serde_tuple::Serialize_tuple`].
//!   This corresponds to the CBOR representation.
//! - Implement [`HasLotusJson`] on the domain object.
//!   This attaches a separate JSON type, which should implement (`#[derive(...)]`) [`serde::Serialize`] and [`serde::Deserialize`] AND conversions to and from the domain object
//!   E.g [`gossip_block`]
//!
//! Whenever you need the lotus JSON of an object, use the [`LotusJson`] wrapper.
//! Note that the actual [`HasLotusJson::LotusJson`] types should be private - we don't want these names
//! proliferating over the codebase.
//!
//! ## Implementation notes
//! ### Illegal states are unrepresentable
//! Consider [Address](crate::shim::address::Address) - it is represented as a simple string in JSON,
//! so there are two possible definitions of `AddressLotusJson`:
//! ```rust
//! # use serde::{Deserialize, Serialize};
//! # #[derive(Serialize, Deserialize)] enum Address {}
//! # mod stringify {
//! #     pub fn serialize<T, S: serde::Serializer>(_: &T, _: S) -> Result<S::Ok, S::Error> { unimplemented!() }
//! #     pub fn deserialize<'de, T, D: serde::Deserializer<'de>>(_: D) -> Result<T, D::Error> { unimplemented!() }
//! # }
//! #[derive(Serialize, Deserialize)]
//! pub struct AddressLotusJson(#[serde(with = "stringify")] Address);
//! ```
//! ```rust
//! # use serde::{Deserialize, Serialize};
//! #[derive(Serialize, Deserialize)]
//! pub struct AddressLotusJson(String);
//! ```
//! However, with the second implementation, `impl From<AddressLotusJson> for Address` would involve unwrapping
//! a call to [std::primitive::str::parse], which is unacceptable - malformed JSON could cause a crash!
//!
//! ### Location
//! Prefer implementing in this module, as [`decl_and_test`] will handle `quickcheck`-ing and snapshot testing.
//!
//! If you require access to private fields, consider:
//! - implementing an exhaustive helper method, e.g [`crate::beacon::BeaconEntry::into_parts`].
//! - moving implementation to the module where the struct is defined, e.g [`crate::blocks::header::lotus_json::BlockHeaderLotusJson`].
//!   If you do this, you MUST manually add snapshot and `quickcheck` tests.
//!
//! ### Compound structs
//! - Each field of a struct should be wrapped with [`LotusJson`].
//! - Implementations of [`HasLotusJson::into_lotus_json`] and [`HasLotusJson::from_lotus_json`]
//!   should use [`Into`] and [`LotusJson::into_inner`] calls
//! - Use destructuring to ensure exhaustiveness
//!
//! ### Optional fields
//! Optional fields must have the following annotations:
//! ```rust,ignore
//! # struct Foo {
//! #[serde(skip_serializing_if = "LotusJson::is_none", default)]
//! foo: LotusJson<Option<usize>>,
//! # }
//! ```
//!
//! # API hazards
//! - Avoid using `#[serde(with = ...)]` except for leaf types
//! - There is a hazard if the same type can be de/serialized in multiple ways.
//!
//! # Future work
//! - use [`proptest`](https://docs.rs/proptest/) to test the parser pipeline
//! - use a derive macro for simple compound structs

use derive_more::From;
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::json;
use std::{fmt::Display, str::FromStr};
#[cfg(test)]
use {pretty_assertions::assert_eq, quickcheck::quickcheck};

pub trait HasLotusJson: Sized {
    /// The struct representing JSON. You should `#[derive(Deserialize, Serialize)]` on it.
    type LotusJson: Serialize + DeserializeOwned;
    /// To ensure code quality, conversion to/from lotus JSON MUST be tested.
    /// Provide snapshots of the JSON, and the domain type it should serialize to.
    ///
    /// Serialization and de-serialization of the domain type should match the snapshot.
    ///
    /// If using [`decl_and_test`], this test is automatically run for you, but if the test
    /// is out-of-module, you must call [`assert_all_snapshots`] manually.
    fn snapshots() -> Vec<(serde_json::Value, Self)>;
    fn into_lotus_json(self) -> Self::LotusJson;
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self;
}

macro_rules! decl_and_test {
    ($($mod_name:ident for $domain_ty:ty),* $(,)?) => {
        $(
            mod $mod_name;
        )*
        #[test]
        fn all_snapshots() {
            $(
                print!("test snapshots for {}...", std::any::type_name::<$domain_ty>());
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
                // ^ make sure the above line is flushed in case the test fails
                assert_all_snapshots::<$domain_ty>();
                println!("ok.");
            )*
        }
        #[test]
        fn all_quickchecks() {
            $(
                print!("quickcheck for {}...", std::any::type_name::<$domain_ty>());
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
                // ^ make sure the above line is flushed in case the test fails
                ::quickcheck::quickcheck(assert_unchanged_via_json::<$domain_ty> as fn(_));
                println!("ok.");
            )*
        }
    }
}
#[cfg(doc)]
pub(crate) use decl_and_test;

decl_and_test!(
    actor_state for crate::shim::state_tree::ActorState,
    address for crate::shim::address::Address,
    beacon_entry for crate::beacon::BeaconEntry,
    big_int for num::BigInt,
    election_proof for crate::blocks::ElectionProof,
    gossip_block for crate::blocks::GossipBlock,
    key_info for crate::key_management::KeyInfo,
    message for crate::shim::message::Message,
    po_st_proof for crate::shim::sector::PoStProof,
    registered_po_st_proof for crate::shim::sector::RegisteredPoStProof,
    registered_seal_proof for crate::shim::sector::RegisteredSealProof,
    sector_info for crate::shim::sector::SectorInfo,
    signature for crate::shim::crypto::Signature,
    signature_type for crate::shim::crypto::SignatureType,
    signed_message for  crate::message::SignedMessage,
    sync_stage for crate::chain_sync::SyncStage,
    ticket for crate::blocks::Ticket,
    tipset_keys for crate::blocks::TipsetKeys,
    token_amount for crate::shim::econ::TokenAmount,
    vec_u8 for Vec<u8>,
    vrf_proof for crate::blocks::VRFProof,
);

mod cid; // can't make snapshots of generic type
mod opt; // can't make snapshots of generic type
mod raw_bytes; // fvm_ipld_encoding::RawBytes: !quickcheck::Arbitrary
mod receipt; // shim type roundtrip is wrong - see module
mod vec; // can't make snapshots of generic type

#[cfg(any(test, doc))]
pub fn assert_all_snapshots<T>()
where
    T: HasLotusJson + PartialEq + std::fmt::Debug + Clone,
{
    let snapshots = T::snapshots();
    assert!(!snapshots.is_empty());
    for (lotus_json, val) in snapshots {
        assert_one_snapshot(lotus_json, val);
    }
}

#[cfg(test)]
pub fn assert_one_snapshot<T>(lotus_json: serde_json::Value, val: T)
where
    T: HasLotusJson + PartialEq + std::fmt::Debug + Clone,
{
    // T -> T::LotusJson -> lotus_json
    let serialized = serde_json::to_value(val.clone().into_lotus_json()).unwrap();
    assert_eq!(
        serialized.to_string(),
        lotus_json.to_string(),
        "snapshot failed for {}",
        std::any::type_name::<T>()
    );

    // lotus_json -> T::LotusJson -> T
    let deserialized = match serde_json::from_value::<T::LotusJson>(lotus_json.clone()) {
        Ok(lotus_json) => T::from_lotus_json(lotus_json),
        Err(e) => panic!(
            "couldn't deserialize a {} from {}: {e}",
            std::any::type_name::<T::LotusJson>(),
            lotus_json
        ),
    };
    assert_eq!(deserialized, val);
}

#[cfg(any(test, doc))]
pub fn assert_unchanged_via_json<T>(val: T)
where
    T: HasLotusJson + Clone + PartialEq + std::fmt::Debug,
    T::LotusJson: Serialize + serde::de::DeserializeOwned,
{
    // T -> T::LotusJson -> lotus_json -> T::LotusJson -> T

    // T -> T::LotusJson
    let temp = val.clone().into_lotus_json();
    // T::LotusJson -> lotus_json
    let temp = serde_json::to_value(temp).unwrap();
    // lotus_json -> T::LotusJson
    let temp = serde_json::from_value::<T::LotusJson>(temp).unwrap();
    // T::LotusJson -> T
    let temp = T::from_lotus_json(temp);

    assert_eq!(val, temp);
}

/// Usage: `#[serde(with = "stringify")]`
pub mod stringify {
    use super::*;

    pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Display,
        S: Serializer,
    {
        serializer.collect_str(value)
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: FromStr,
        T::Err: Display,
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// Usage: `#[serde(with = "base64_standard")]`
pub mod base64_standard {
    use super::*;

    use base64::engine::{general_purpose::STANDARD, Engine as _};

    pub fn serialize<S>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        STANDARD.encode(value).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        STANDARD
            .decode(String::deserialize(deserializer)?)
            .map_err(serde::de::Error::custom)
    }
}

/// MUST NOT be used in any `LotusJson` structs
pub fn serialize<S, T>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: HasLotusJson + Clone,
{
    value.clone().into_lotus_json().serialize(serializer)
}

/// MUST NOT be used in any `LotusJson` structs.
pub fn deserialize<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: HasLotusJson,
{
    Ok(T::from_lotus_json(Deserialize::deserialize(deserializer)?))
}

/// A domain struct that is (de) serialized through its lotus JSON representation.
#[derive(Serialize, Deserialize, From, Default)]
#[serde(bound = "T: HasLotusJson + Clone")]
pub struct LotusJson<T>(#[serde(with = "self")] pub T);

impl<T> LotusJson<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> LotusJson<Option<T>> {
    // don't want to impl Deref<T> for LotusJson<T>
    pub fn is_none(&self) -> bool {
        self.0.is_none()
    }
}

/// A struct that is (de) serialized through its [`Display`] and [`FromStr`] implementations.
#[derive(Serialize, Deserialize, From, Default)]
#[serde(bound = "T: Display + FromStr, T::Err: Display")]
pub struct Stringify<T>(#[serde(with = "stringify")] pub T);

impl<T> Stringify<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

macro_rules! lotus_json_with_self {
    ($($domain_ty:ty),* $(,)?) => {
        $(
            impl HasLotusJson for $domain_ty {
                type LotusJson = Self;
                fn snapshots() -> Vec<(serde_json::Value, Self)> {
                    unimplemented!("tests are trivial for HasLotusJson<LotusJson = Self>")
                }
                fn into_lotus_json(self) -> Self::LotusJson {
                    self
                }
                fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
                    lotus_json
                }
            }
        )*
    }
}

lotus_json_with_self!(u32, u64, i64, String, chrono::DateTime<chrono::Utc>);
