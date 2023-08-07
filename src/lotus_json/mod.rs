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
//! The way forest currently handles the two is to have a single struct represent
//! - the domain object
//! - the in memory representation
//! - the lotus cbor via `#[derive(Serialize, Deserialize)]`
//!
//! This is largely the right decision - the [Serialize] and [Deserialize]
//! implementations of crates we depend on model the lotus cbor only.
//!
//! However, the way we handle json is inconsistent:
//! - [Typically we create a `json` module for the domain object, for use with `serde(with = ...)`](https://github.com/ChainSafe/forest/blob/77d6b2b128d73900b0162e3f573ff8d63e6324b3/src/json/token_amount.rs)
//! - [Sometimes we create a `FooJson` wrapper struct to wrap the deserialization](https://github.com/ChainSafe/forest/blob/77d6b2b128d73900b0162e3f573ff8d63e6324b3/src/json/message_receipt.rs#L17)
//! - [Sometimes we create a `JsonHelper` struct for serde](https://github.com/ChainSafe/forest/blob/77d6b2b128d73900b0162e3f573ff8d63e6324b3/src/json/signature.rs#L20-L25)
//! - Sometimes we create [different](https://github.com/ChainSafe/forest/blob/77d6b2b128d73900b0162e3f573ff8d63e6324b3/src/blocks/header/json.rs#L37-L66) [structs](https://github.com/ChainSafe/forest/blob/77d6b2b128d73900b0162e3f573ff8d63e6324b3/src/blocks/header/json.rs#L95-L124) for each serialization direction,
//!   where one typically wraps a reference.
//! - Typically we create additional [vec](https://github.com/ChainSafe/forest/blob/77d6b2b128d73900b0162e3f573ff8d63e6324b3/src/json/cid.rs#L45-L78) and [opt](https://github.com/ChainSafe/forest/blob/77d6b2b128d73900b0162e3f573ff8d63e6324b3/src/json/cid.rs#L80-L99) modules for domain objects which may be wrapped as `Vec<T>` and `Option<T>`
//!
//! This PR explores more structured ways to handle this.
//!
//! # How about a shadow tree with [HasLotusJson]?
//!
//! JSON input is on the slow path for forest - we don't expect large numbers of RPC API calls.
//! We can create mirror `LotusJson` versions of the required structs - most of the complexity above goes away.
//! We then ensure [From] and [Into] conversions to the domain objects.
//! With careful design, this could theoretically be a `#[derive(LotusJson)]` macro in future.
//!
//! # How about a custom trait which represents both with [LotusSerialize] and [LotusDeserialize]?
//!
//! # How about a witness type like `LotusJsonDeser<T>(T)`?
//!
//! # We know all the weird types - can we do downcast magic?

use derive_more::{From, Into};
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::json;
use std::{fmt::Display, str::FromStr};
#[cfg(test)]
use {pretty_assertions::assert_eq, quickcheck::quickcheck};

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

pub trait HasLotusJson: Sized + Into<Self::LotusJson> {
    type LotusJson: Into<Self> + Serialize + DeserializeOwned;
    fn snapshots() -> Vec<(serde_json::Value, Self)>;
}

#[cfg(test)]
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
    let serialized = serde_json::to_value(Into::<T::LotusJson>::into(val.clone())).unwrap();
    assert_eq!(
        serialized.to_string(),
        lotus_json.to_string(),
        "snapshot failed for {}",
        std::any::type_name::<T>()
    );

    // lotus_json -> T::LotusJson -> T
    let deserialized = match serde_json::from_value::<T::LotusJson>(lotus_json.clone()) {
        Ok(lotus_json) => Into::<T>::into(lotus_json),
        Err(e) => panic!(
            "couldn't deserialize a {} from {}: {e}",
            std::any::type_name::<T::LotusJson>(),
            lotus_json
        ),
    };
    assert_eq!(deserialized, val);
}

#[cfg(test)]
pub fn assert_unchanged_via_json<T>(val: T)
where
    T: HasLotusJson + Clone + Into<T::LotusJson> + PartialEq + std::fmt::Debug,
    T::LotusJson: Into<T> + Serialize + serde::de::DeserializeOwned,
{
    // T -> T::LotusJson -> lotus_json -> T::LotusJson -> T

    // T -> T::LotusJson
    let temp = Into::<T::LotusJson>::into(val.clone());
    // T::LotusJson -> lotus_json
    let temp = serde_json::to_value(temp).unwrap();
    // lotus_json -> T::LotusJson
    let temp = serde_json::from_value::<T::LotusJson>(temp).unwrap();
    // T::LotusJson -> T
    let temp = Into::<T>::into(temp);

    assert_eq!(val, temp);
}

// TODO(aatifsyed): we should be able to write quickchecks that make sure our parser pipeline doesn't panic
// but quickcheck is not powerful enough... we should use proptest instead/in addition

macro_rules! decl_and_test {
    ($($mod_name:ident -> $lotus_json_ty:ident for $domain_ty:ty),* $(,)?) => {
        $(
            #[allow(unused)]
            pub use self::$mod_name::$lotus_json_ty;
            mod $mod_name;
        )*
        #[test]
        fn all_snapshots() {
            $(
                print!("test snapshots for {}...", std::any::type_name::<$domain_ty>());
                assert_all_snapshots::<$domain_ty>();
                println!("ok.");
            )*
        }
        #[test]
        fn all_quickchecks() {
            $(
                print!("quickcheck for {}...", std::any::type_name::<$domain_ty>());
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
                // ^ make sure the above line is flushed in the case that the quickcheck fails
                quickcheck::quickcheck(assert_unchanged_via_json::<$domain_ty> as fn(_));
                println!("ok.");
            )*
        }
    }
}

decl_and_test!(
    address -> AddressLotusJson for crate::shim::address::Address,
    beacon_entry -> BeaconEntryLotusJson for crate::beacon::BeaconEntry,
    big_int -> BigIntLotusJson for num::BigInt,
    gossip_block -> GossipBlockLotusJson for crate::blocks::GossipBlock,
    election_proof -> ElectionProofLotusJson for crate::blocks::ElectionProof,
    message -> MessageLotusJson for crate::shim::message::Message,
    po_st_proof -> PoStProofLotusJson for crate::shim::sector::PoStProof,
    registered_po_st_proof -> RegisteredPoStProofLotusJson for crate::shim::sector::RegisteredPoStProof,
    signature -> SignatureLotusJson for crate::shim::crypto::Signature,
    signature_type -> SignatureTypeLotusJson for crate::shim::crypto::SignatureType,
    signed_message -> SignedMessageLotusJson for  crate::message::SignedMessage,
    ticket -> TicketLotusJson for crate::blocks::Ticket,
    tipset_keys ->  TipsetKeysLotusJson for crate::blocks::TipsetKeys,
    token_amount -> TokenAmountLotusJson for crate::shim::econ::TokenAmount,
    vec_u8 -> VecU8LotusJson for Vec<u8>,
    vrf_proof -> VRFProofLotusJson for  crate::json::vrf::VRFProof, // TODO(aatifsyed): why is this in `json`?
);

pub use self::cid::CidLotusJson;
mod cid; // can't make snapshots of generic type

pub use self::vec::VecLotusJson;
mod vec; // can't make snapshots of generic type

pub use self::raw_bytes::RawBytesLotusJson;
mod raw_bytes; // fvm_ipld_encoding::RawBytes: !quickcheck::Arbitrary

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

#[cfg(test)]
pub fn deserialize<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: HasLotusJson,
{
    T::LotusJson::deserialize(deserializer).map(Into::into)
}
