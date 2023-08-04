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
//! - the lotus cbor
//!
//! This is largely the right decision - the [serde::Serialize] and [serde::Deserialize]
//! implementations of crates we depend on model the lotus cbor only.
//!
//! # How about a shadow tree with [HasLotusJson]?
//!
//! # How about a custom trait which represents both with [LotusSerialize] and [LotusDeserialize]?
//!
//! # How about a witness type like `LotusJsonDeser<T>(T)`?
//!
//! # We know all the weird types - can we do downcast magic?

use derive_more::{From, Into};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{fmt::Display, str::FromStr};
#[cfg(test)]
use {quickcheck::quickcheck, serde_json::json};

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

pub trait HasLotusJson {
    type LotusJson;
}

#[cfg(test)]
fn assert_snapshot</* 'de, */ T>(lotus_json: serde_json::Value, val: T)
where
    // API
    T: HasLotusJson,
    T::LotusJson: Serialize, // + Deserialize<'de>, // conflicts with DeserializeOwned
    T::LotusJson: Into<T>,
    T: Into<T::LotusJson>,
    // Testing
    T: PartialEq + std::fmt::Debug,
    T::LotusJson: serde::de::DeserializeOwned,
{
    // lotus_json -> T::LotusJson -> T
    let deserialized =
        Into::<T>::into(serde_json::from_value::<T::LotusJson>(lotus_json.clone()).unwrap());
    assert_eq!(deserialized, val);

    // T -> T::LotusJson -> lotus_json
    let serialized = serde_json::to_value(Into::<T::LotusJson>::into(val)).unwrap();
    assert_eq!(serialized, lotus_json);
}

#[cfg(test)]
fn assert_via_json<T>(val: T)
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

use self::cid::CidLotusJson;
mod cid {
    use super::*;

    pub type CidLotusJson = CidLotusJsonGeneric<64>;

    #[derive(Serialize, Deserialize, From, Into)]
    pub struct CidLotusJsonGeneric<const S: usize> {
        #[serde(rename = "/", with = "stringify")]
        slash: ::cid::CidGeneric<S>,
    }

    impl<const S: usize> HasLotusJson for ::cid::CidGeneric<S> {
        type LotusJson = CidLotusJsonGeneric<S>;
    }

    #[test]
    fn test() {
        assert_snapshot(json!({"/": "baeaaaaa"}), ::cid::Cid::default());
    }

    #[cfg(test)]
    quickcheck! {
        fn round_trip(val: ::cid::Cid) -> bool {
            assert_via_json(val);
            true
        }
    }
}

use token_amount::TokenAmountLotusJson;
mod token_amount {
    use super::*;

    use crate::shim::econ::TokenAmount;

    #[derive(Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct TokenAmountLotusJson {
        #[serde(with = "stringify")]
        attos: num::BigInt,
    }

    impl HasLotusJson for TokenAmount {
        type LotusJson = TokenAmountLotusJson;
    }

    impl From<TokenAmount> for TokenAmountLotusJson {
        fn from(value: TokenAmount) -> Self {
            Self {
                attos: value.atto().clone(),
            }
        }
    }

    impl From<TokenAmountLotusJson> for TokenAmount {
        fn from(value: TokenAmountLotusJson) -> Self {
            Self::from_atto(value.attos)
        }
    }

    #[test]
    fn test() {
        assert_snapshot(json!("1"), TokenAmount::from_atto(1));
    }

    #[cfg(test)]
    quickcheck! {
        fn round_trip(val: TokenAmount) -> bool {
            assert_via_json(val);
            true
        }
    }
}

use address::AddressLotusJson;
mod address {
    use super::*;

    use crate::shim::address::Address;

    #[derive(Serialize, Deserialize, From, Into)]
    #[serde(transparent)]
    pub struct AddressLotusJson(#[serde(with = "stringify")] Address);

    impl HasLotusJson for Address {
        type LotusJson = AddressLotusJson;
    }

    #[test]
    fn test() {
        assert_snapshot(json!("f00"), Address::default());
    }

    #[cfg(test)]
    quickcheck! {
        fn round_trip(val: Address) -> bool {
            assert_via_json(val);
            true
        }
    }
}

use message::MessageLotusJson;
mod message {
    use super::{raw_bytes::RawBytesLotusJson, *};

    use crate::shim::message::Message;

    // TODO(aatifsyed): derive
    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct MessageLotusJson {
        version: u64,
        to: AddressLotusJson,
        from: AddressLotusJson,
        nonce: u64,
        value: TokenAmountLotusJson,
        gas_limit: u64,
        gas_fee_cap: TokenAmountLotusJson,
        gas_premium: TokenAmountLotusJson,
        method: u64,
        params: Option<RawBytesLotusJson>,
        #[serde(rename = "CID", skip_serializing_if = "Option::is_none")]
        cid: Option<CidLotusJson>,
    }

    impl HasLotusJson for Message {
        type LotusJson = MessageLotusJson;
    }

    // TODO(aatifsyed): derive
    impl From<MessageLotusJson> for Message {
        fn from(value: MessageLotusJson) -> Self {
            let MessageLotusJson {
                version,
                to,
                from,
                nonce,
                value,
                gas_limit,
                gas_fee_cap,
                gas_premium,
                method,
                params,
                cid: _ignored, // TODO(aatifsyed): is this an error?
            } = value;
            Self {
                version,
                from: from.into(),
                to: to.into(),
                sequence: nonce,
                value: value.into(),
                method_num: method,
                params: params.map(Into::into).unwrap_or_default(),
                gas_limit,
                gas_fee_cap: gas_fee_cap.into(),
                gas_premium: gas_premium.into(),
            }
        }
    }

    impl From<Message> for MessageLotusJson {
        fn from(value: Message) -> Self {
            let Message {
                version,
                from,
                to,
                sequence,
                value,
                method_num,
                params,
                gas_limit,
                gas_fee_cap,
                gas_premium,
            } = value;
            Self {
                version,
                to: to.into(),
                from: from.into(),
                nonce: sequence,
                value: value.into(),
                gas_limit,
                gas_fee_cap: gas_fee_cap.into(),
                gas_premium: gas_premium.into(),
                method: method_num,
                params: Some(params.into()),
                cid: None, // TODO(aatifsyed): is this an error?
            }
        }
    }

    #[test]
    fn test() {
        assert_snapshot(
            json!({
                "From": "f00",
                "GasFeeCap": "0",
                "GasLimit": 0, // BUG?(aatifsyed)
                "GasPremium": "0",
                "Method": 0,
                "Nonce": 0,
                "Params": "", // BUG?(aatifsyed)
                "To": "f00",
                "Value": "0",
                "Version": 0
            }),
            Message::default(),
        );
    }

    #[cfg(test)]
    quickcheck! {
        fn round_trip(val: Message) -> bool {
            assert_via_json(val);
            true
        }
    }
}

use signature_type::SignatureTypeLotusJson;
mod signature_type {
    use super::*;
    use crate::shim::crypto::SignatureType;

    #[derive(Deserialize, Serialize)]
    #[serde(rename_all = "lowercase")]
    pub enum SignatureTypeLotusJson {
        Bls,
        Secp256k1,
        Delegated,
    }

    impl HasLotusJson for SignatureType {
        type LotusJson = SignatureTypeLotusJson;
    }

    impl From<SignatureTypeLotusJson> for SignatureType {
        fn from(value: SignatureTypeLotusJson) -> Self {
            match value {
                SignatureTypeLotusJson::Bls => Self::Bls,
                SignatureTypeLotusJson::Secp256k1 => Self::Secp256k1,
                SignatureTypeLotusJson::Delegated => Self::Delegated,
            }
        }
    }

    impl From<SignatureType> for SignatureTypeLotusJson {
        fn from(value: SignatureType) -> Self {
            match value {
                SignatureType::Secp256k1 => Self::Secp256k1,
                SignatureType::Bls => Self::Bls,
                SignatureType::Delegated => Self::Delegated,
            }
        }
    }

    #[test]
    fn test() {
        assert_snapshot(json!("bls"), SignatureType::Bls);
    }

    #[cfg(test)]
    quickcheck! {
        fn round_trip(val: SignatureType) -> bool {
            assert_via_json(val);
            true
        }
    }
}

use signature::SignatureLotusJson;
mod signature {
    use super::*;
    use crate::shim::crypto::Signature;

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct SignatureLotusJson {
        r#type: SignatureTypeLotusJson,
        #[serde(with = "base64_standard")]
        data: Vec<u8>,
    }

    impl HasLotusJson for Signature {
        type LotusJson = SignatureLotusJson;
    }

    impl From<SignatureLotusJson> for Signature {
        fn from(value: SignatureLotusJson) -> Self {
            let SignatureLotusJson { r#type, data } = value;
            Self {
                sig_type: r#type.into(),
                bytes: data,
            }
        }
    }

    impl From<Signature> for SignatureLotusJson {
        fn from(value: Signature) -> Self {
            let Signature { sig_type, bytes } = value;
            Self {
                r#type: sig_type.into(),
                data: bytes,
            }
        }
    }

    #[test]
    fn test() {
        assert_snapshot(
            json!({"Type": "bls", "Data": "aGVsbG8gd29ybGQh"}),
            Signature {
                sig_type: crate::shim::crypto::SignatureType::Bls,
                bytes: Vec::from_iter(*b"hello world!"),
            },
        );
    }

    #[cfg(test)]
    quickcheck! {
        fn round_trip(val: Signature) -> bool {
            assert_via_json(val);
            true
        }
    }
}

use signed_message::SignedMessageLotusJson;
mod signed_message {
    use crate::message::SignedMessage;

    use super::*;

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct SignedMessageLotusJson {
        message: MessageLotusJson,
        signature: SignatureLotusJson,
        #[serde(rename = "CID", skip_serializing_if = "Option::is_none")]
        cid: Option<CidLotusJson>,
    }

    impl HasLotusJson for SignedMessage {
        type LotusJson = SignedMessageLotusJson;
    }

    impl From<SignedMessage> for SignedMessageLotusJson {
        fn from(value: SignedMessage) -> Self {
            let SignedMessage { message, signature } = value;
            Self {
                message: message.into(),
                signature: signature.into(),
                cid: None, // BUG?(aatifsyed)
            }
        }
    }

    impl From<SignedMessageLotusJson> for SignedMessage {
        fn from(value: SignedMessageLotusJson) -> Self {
            let SignedMessageLotusJson {
                message,
                signature,
                cid: _ignored, // BUG?(aatifsyed)
            } = value;
            Self {
                message: message.into(),
                signature: signature.into(),
            }
        }
    }

    #[test]
    fn test() {
        assert_snapshot(
            json!({
                "Message": {
                    "From": "f00",
                    "GasFeeCap": "0",
                    "GasLimit": 0,
                    "GasPremium": "0",
                    "Method": 0,
                    "Nonce": 0,
                    "Params": "",
                    "To": "f00",
                    "Value": "0",
                    "Version": 0
                },
                "Signature": {"Type": "bls", "Data": "aGVsbG8gd29ybGQh"}
            }),
            SignedMessage {
                message: crate::shim::message::Message::default(),
                signature: crate::shim::crypto::Signature {
                    sig_type: crate::shim::crypto::SignatureType::Bls,
                    bytes: Vec::from_iter(*b"hello world!"),
                },
            },
        );
    }

    #[cfg(test)]
    quickcheck! {
        fn round_trip(val: SignedMessage) -> bool {
            assert_via_json(val);
            true
        }
    }
}

mod raw_bytes {
    use super::*;
    use fvm_ipld_encoding::RawBytes;

    #[derive(Serialize, Deserialize)]
    #[serde(transparent)]
    pub struct RawBytesLotusJson(#[serde(with = "base64_standard")] Vec<u8>);

    impl HasLotusJson for RawBytes {
        type LotusJson = RawBytesLotusJson;
    }

    impl From<RawBytes> for RawBytesLotusJson {
        fn from(value: RawBytes) -> Self {
            RawBytesLotusJson(Vec::from(value))
        }
    }

    impl From<RawBytesLotusJson> for RawBytes {
        fn from(value: RawBytesLotusJson) -> Self {
            Self::from(value.0)
        }
    }

    #[test]
    fn test() {
        assert_snapshot(
            json!("aGVsbG8gd29ybGQh"),
            RawBytes::new(Vec::from_iter(*b"hello world!")),
        );
    }

    #[cfg(test)]
    quickcheck! {
        fn round_trip(val: Vec<u8>) -> bool {
            assert_via_json(RawBytes::new(val));
            true
        }
    }
}

mod vec {
    use super::*;

    pub struct VecLotusJson<T>(Vec<T>);

    impl<T> HasLotusJson for Vec<T>
    where
        T: HasLotusJson,
    {
        type LotusJson = VecLotusJson<T::LotusJson>;
    }

    impl<T> Serialize for VecLotusJson<T> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            match self.0.is_empty() {
                true => serializer.serialize_none(),
                false => self.serialize(serializer),
            }
        }
    }

    impl<'de, T> Deserialize<'de> for VecLotusJson<T>
    where
        T: Deserialize<'de>,
    {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            Option::<Vec<T>>::deserialize(deserializer)
                .map(Option::unwrap_or_default)
                .map(Self)
        }
    }

    // VecLotusJson<T::LotusJson> -> Vec<T>
    impl<T> From<VecLotusJson<T::LotusJson>> for Vec<T>
    where
        T: HasLotusJson,
        T::LotusJson: Into<T>,
    {
        fn from(value: VecLotusJson<T::LotusJson>) -> Self {
            value.0.into_iter().map(Into::into).collect()
        }
    }

    // Vec<T> -> VecLotusJson<T::LotusJson>
    impl<T> From<Vec<T>> for VecLotusJson<T::LotusJson>
    where
        T: HasLotusJson + Into<T::LotusJson>,
    {
        fn from(value: Vec<T>) -> Self {
            Self(value.into_iter().map(Into::into).collect())
        }
    }

    #[test]
    fn test() {
        assert_snapshot(json!([{"/": "baeaaaaa"}]), vec![::cid::Cid::default()]);
    }

    #[cfg(test)]
    quickcheck! {
        fn round_trip(val: Vec<::cid::Cid>) -> bool {
            assert_via_json(val);
            true
        }
    }
}

/// Usage: `#[serde(with = "stringify")]`
mod stringify {
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

mod base64_standard {
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
