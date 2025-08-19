// Copyright 2019-2025 ChainSafe Systems
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
//! - Implement [`serde::Serialize`] on that object, normally using [`fvm_ipld_encoding::tuple::Serialize_tuple`].
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
//! - moving implementation to the module where the struct is defined, e.g [`crate::blocks::tipset::lotus_json`].
//!   If you do this, you MUST manually add snapshot and `quickcheck` tests.
//!
//! ### Compound structs
//! - Each field of a struct should be wrapped with [`LotusJson`].
//! - Implementations of [`HasLotusJson::into_lotus_json`] and [`HasLotusJson::from_lotus_json`]
//!   should use [`Into`] and [`LotusJson::into_inner`] calls
//! - Use destructuring to ensure exhaustiveness
//!
//! ### Optional fields
//! It's not clear if optional fields should be serialized as `null` or not.
//! See e.g `LotusJson<Receipt>`.
//!
//! For now, fields are recommended to have the following annotations:
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

use crate::shim::actors::miner::DeadlineInfo;
use derive_more::From;
use fvm_shared4::piece::PaddedPieceSize;
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::DeserializeOwned};
#[cfg(test)]
use serde_json::json;
use std::{fmt::Display, str::FromStr};
use uuid::Uuid;
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
    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)>;
    fn into_lotus_json(self) -> Self::LotusJson;
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self;
    fn into_lotus_json_value(self) -> serde_json::Result<serde_json::Value> {
        serde_json::to_value(self.into_lotus_json())
    }
    fn into_lotus_json_string(self) -> serde_json::Result<String> {
        serde_json::to_string(&self.into_lotus_json())
    }
    fn into_lotus_json_string_pretty(self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(&self.into_lotus_json())
    }
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
    block_header for crate::blocks::CachingBlockHeader,
    cid for ::cid::Cid,
    duration for std::time::Duration,
    election_proof for crate::blocks::ElectionProof,
    extended_sector_info for crate::shim::sector::ExtendedSectorInfo,
    gossip_block for crate::blocks::GossipBlock,
    key_info for crate::key_management::KeyInfo,
    message for crate::shim::message::Message,
    po_st_proof for crate::shim::sector::PoStProof,
    registered_po_st_proof for crate::shim::sector::RegisteredPoStProof,
    registered_seal_proof for crate::shim::sector::RegisteredSealProof,
    sector_info for crate::shim::sector::SectorInfo,
    sector_size for crate::shim::sector::SectorSize,
    signature for crate::shim::crypto::Signature,
    signature_type for crate::shim::crypto::SignatureType,
    signed_message for  crate::message::SignedMessage,
    ticket for crate::blocks::Ticket,
    tipset_keys for crate::blocks::TipsetKey,
    token_amount for crate::shim::econ::TokenAmount,
    vec_u8 for Vec<u8>,
    vrf_proof for crate::blocks::VRFProof,
);

// If a module cannot be tested normally above, you MAY declare it separately here
// but you MUST document any tech debt - the reason WHY it cannot be tested above.
mod actor_states;
mod allocation;
mod beneficiary_term; // fil_actor_miner_state::v12::BeneficiaryTerm: !quickcheck::Arbitrary
mod bit_field; //  fil_actors_shared::fvm_ipld_bitfield::BitField: !quickcheck::Arbitrary
mod bytecode_hash;
mod entry;
mod filter_estimate;
mod hash_map;
mod ipld; // NaN != NaN
mod miner_info; // fil_actor_miner_state::v12::MinerInfo: !quickcheck::Arbitrary
mod miner_power; // actors::miner::MinerInfo: !quickcheck::Arbitrary
mod nonempty; // can't make snapshots of generic type
mod opt; // can't make snapshots of generic type
mod pending_beneficiary_change; // fil_actor_miner_state::v12::PendingBeneficiaryChange: !quickcheck::Arbitrary
mod power_claim; // actors::power::Claim: !quickcheck::Arbitrary
mod raw_bytes; // fvm_ipld_encoding::RawBytes: !quickcheck::Arbitrary
mod receipt; // shim type roundtrip is wrong - see module
mod token_state;
mod tombstone;
mod transient_data;
mod vec; // can't make snapshots of generic type
mod verifreg_claim;

pub use vec::*;

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
    let serialized = val.clone().into_lotus_json_value().unwrap();
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

/// Usage: `#[serde(with = "hexify_bytes")]`
pub mod hexify_bytes {
    use super::*;

    pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Display + std::fmt::LowerHex,
        S: Serializer,
    {
        // `ethereum_types` crate serializes bytes as compressed addresses, i.e. `0xff00…03ec`
        // so we can't just use `serializer.collect_str` here
        serializer.serialize_str(&format!("{value:#x}"))
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

pub mod hexify_vec_bytes {
    use super::*;
    use std::borrow::Cow;

    pub fn serialize<S>(value: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = vec![0; 2 + value.len() * 2];
        s.get_mut(0..2)
            .expect("len is correct")
            .copy_from_slice(b"0x");
        hex::encode_to_slice(value, s.get_mut(2..).expect("len is correct"))
            .map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(std::str::from_utf8(&s).expect("valid utf8"))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let s = Cow::from(s.strip_prefix("0x").unwrap_or(&s));

        // Pad with 0 if odd length. This is necessary because [`hex::decode`] requires an even
        // number of characters, whereas a valid input is also `0x0`.
        let s = if s.len() % 2 == 0 {
            s
        } else {
            let mut s = s.into_owned();
            s.insert(0, '0');
            Cow::Owned(s)
        };

        hex::decode(s.as_ref()).map_err(serde::de::Error::custom)
    }
}

/// Usage: `#[serde(with = "hexify")]`
pub mod hexify {
    use super::*;
    use num_traits::Num;
    use serde::{Deserializer, Serializer};

    pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        T: Num + std::fmt::LowerHex,
        S: Serializer,
    {
        serializer.serialize_str(format!("{value:#x}").as_str())
    }

    pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
    where
        T: Num,
        <T as Num>::FromStrRadixErr: std::fmt::Display,
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        #[allow(clippy::indexing_slicing)]
        if s.len() > 2 && &s[..2] == "0x" {
            T::from_str_radix(&s[2..], 16).map_err(serde::de::Error::custom)
        } else {
            Err(serde::de::Error::custom("Invalid hex"))
        }
    }
}

/// Usage: `#[serde(with = "base64_standard")]`
pub mod base64_standard {
    use super::*;

    use base64::engine::{Engine as _, general_purpose::STANDARD};

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
#[derive(
    Debug, Deserialize, From, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Clone,
)]
#[serde(bound = "T: HasLotusJson + Clone", transparent)]
pub struct LotusJson<T>(#[serde(with = "self")] pub T);

impl<T> JsonSchema for LotusJson<T>
where
    T: HasLotusJson,
    T::LotusJson: JsonSchema,
{
    fn schema_name() -> std::borrow::Cow<'static, str> {
        T::LotusJson::schema_name()
    }

    fn schema_id() -> std::borrow::Cow<'static, str> {
        T::LotusJson::schema_id()
    }

    fn json_schema(g: &mut SchemaGenerator) -> Schema {
        T::LotusJson::json_schema(g)
    }
}

impl<T> LotusJson<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

macro_rules! lotus_json_with_self {
    ($($domain_ty:ty),* $(,)?) => {
        $(
            impl $crate::lotus_json::HasLotusJson for $domain_ty {
                type LotusJson = Self;
                #[cfg(test)]
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
pub(crate) use lotus_json_with_self;

lotus_json_with_self!(
    u32,
    u64,
    i64,
    f64,
    String,
    chrono::DateTime<chrono::Utc>,
    serde_json::Value,
    (),
    std::path::PathBuf,
    bool,
    DeadlineInfo,
    PaddedPieceSize,
    Uuid,
);

mod fixme {
    use super::*;

    impl<T: HasLotusJson> HasLotusJson for (T,) {
        type LotusJson = (T::LotusJson,);
        #[cfg(test)]
        fn snapshots() -> Vec<(serde_json::Value, Self)> {
            unimplemented!("tests are trivial for HasLotusJson<LotusJson = Self>")
        }
        fn into_lotus_json(self) -> Self::LotusJson {
            (self.0.into_lotus_json(),)
        }
        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
            (HasLotusJson::from_lotus_json(lotus_json.0),)
        }
    }

    impl<A: HasLotusJson, B: HasLotusJson> HasLotusJson for (A, B) {
        type LotusJson = (A::LotusJson, B::LotusJson);
        #[cfg(test)]
        fn snapshots() -> Vec<(serde_json::Value, Self)> {
            unimplemented!("tests are trivial for HasLotusJson<LotusJson = Self>")
        }
        fn into_lotus_json(self) -> Self::LotusJson {
            (self.0.into_lotus_json(), self.1.into_lotus_json())
        }
        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
            (
                HasLotusJson::from_lotus_json(lotus_json.0),
                HasLotusJson::from_lotus_json(lotus_json.1),
            )
        }
    }

    impl<A: HasLotusJson, B: HasLotusJson, C: HasLotusJson> HasLotusJson for (A, B, C) {
        type LotusJson = (A::LotusJson, B::LotusJson, C::LotusJson);
        #[cfg(test)]
        fn snapshots() -> Vec<(serde_json::Value, Self)> {
            unimplemented!("tests are trivial for HasLotusJson<LotusJson = Self>")
        }
        fn into_lotus_json(self) -> Self::LotusJson {
            (
                self.0.into_lotus_json(),
                self.1.into_lotus_json(),
                self.2.into_lotus_json(),
            )
        }
        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
            (
                HasLotusJson::from_lotus_json(lotus_json.0),
                HasLotusJson::from_lotus_json(lotus_json.1),
                HasLotusJson::from_lotus_json(lotus_json.2),
            )
        }
    }

    impl<A: HasLotusJson, B: HasLotusJson, C: HasLotusJson, D: HasLotusJson> HasLotusJson
        for (A, B, C, D)
    {
        type LotusJson = (A::LotusJson, B::LotusJson, C::LotusJson, D::LotusJson);
        #[cfg(test)]
        fn snapshots() -> Vec<(serde_json::Value, Self)> {
            unimplemented!("tests are trivial for HasLotusJson<LotusJson = Self>")
        }
        fn into_lotus_json(self) -> Self::LotusJson {
            (
                self.0.into_lotus_json(),
                self.1.into_lotus_json(),
                self.2.into_lotus_json(),
                self.3.into_lotus_json(),
            )
        }
        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
            (
                HasLotusJson::from_lotus_json(lotus_json.0),
                HasLotusJson::from_lotus_json(lotus_json.1),
                HasLotusJson::from_lotus_json(lotus_json.2),
                HasLotusJson::from_lotus_json(lotus_json.3),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ipld_core::serde::SerdeError;
    use serde::de::{IntoDeserializer, value::StringDeserializer};

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct HexifyVecBytesTest {
        #[serde(with = "hexify_vec_bytes")]
        value: Vec<u8>,
    }

    #[test]
    fn test_hexify_vec_bytes_serialize() {
        let cases = [(vec![], "0x"), (vec![0], "0x00"), (vec![42, 66], "0x2a42")];

        for (input, expected) in cases.into_iter() {
            let hexify = HexifyVecBytesTest { value: input };
            let serialized = serde_json::to_string(&hexify).unwrap();
            self::assert_eq!(serialized, format!("{{\"value\":\"{}\"}}", expected));
        }
    }

    #[test]
    fn test_hexify_vec_bytes_deserialize() {
        let cases = [
            ("0x", vec![]),
            ("0x0", vec![0]),
            ("0xF", vec![15]),
            ("0x2a42", vec![42, 66]),
            ("0x2A42", vec![42, 66]),
        ];

        for (input, expected) in cases.into_iter() {
            let deserializer: StringDeserializer<SerdeError> =
                String::from_str(input).unwrap().into_deserializer();
            let deserialized = hexify_vec_bytes::deserialize(deserializer).unwrap();
            self::assert_eq!(deserialized, expected);
        }

        let fail_cases = ["cthulhu", "x", "0xazathoth"];
        for input in fail_cases.into_iter() {
            let deserializer: StringDeserializer<SerdeError> =
                String::from_str(input).unwrap().into_deserializer();
            let deserialized = hexify_vec_bytes::deserialize(deserializer);
            assert!(deserialized.is_err());
        }
    }
}
