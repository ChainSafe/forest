// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use fvm_ipld_encoding::{RawBytes, de::Deserializer, ser::Serializer};
use fvm_shared2::message::Message as Message_v2;
pub use fvm_shared3::METHOD_SEND;
pub use fvm_shared3::message::Message as Message_v3;
pub use fvm_shared4::message::Message as Message_v4;
use get_size2::GetSize;
use serde::{Deserialize, Serialize};

use crate::shim::{address::Address, econ::TokenAmount};
use crate::utils::cid::{EncodedCbor, Memo};
use crate::utils::get_size::raw_bytes_heap_size_helper;

/// Method number indicator for calling actor methods.
pub type MethodNum = u64;

#[derive(Clone, Default, PartialEq, Eq, Debug, Hash, GetSize, derive_builder::Builder)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
#[builder(
    default,
    pattern = "owned",
    build_fn(private, name = "build_infallible", error = "std::convert::Infallible")
)]
pub struct Message {
    version: u64,
    from: Address,
    to: Address,
    sequence: u64,
    value: TokenAmount,
    method_num: MethodNum,
    #[cfg_attr(test, arbitrary(gen(
        |g| RawBytes::new(Vec::arbitrary(g))
    )))]
    #[get_size(size_fn = raw_bytes_heap_size_helper)]
    params: RawBytes,
    gas_limit: u64,
    gas_fee_cap: TokenAmount,
    gas_premium: TokenAmount,
    /// The memoized [`Message::cid`] and encoded length, which validation asks for repeatedly
    /// for the same message. Cleared by every setter, so it cannot outlive the fields it was
    /// derived from, and excluded from the derived `PartialEq`/`Eq`/`Hash`/`Debug` by [`Memo`].
    #[builder(setter(skip))]
    #[cfg_attr(test, arbitrary(gen(|_| Memo::default())))]
    encoded: Memo,
}

impl crate::message::MessageRead for Message {
    fn vm_message(&self) -> &Message {
        self
    }
    fn chain_length(&self) -> anyhow::Result<usize> {
        Ok(self.encoded_len())
    }
    fn from(&self) -> Address {
        self.from
    }
    fn to(&self) -> Address {
        self.to
    }
    fn sequence(&self) -> u64 {
        self.sequence
    }
    fn value(&self) -> &TokenAmount {
        &self.value
    }
    fn gas_limit(&self) -> u64 {
        self.gas_limit
    }
    fn required_funds(&self) -> TokenAmount {
        &self.gas_fee_cap * self.gas_limit
    }
    fn gas_fee_cap(&self) -> &TokenAmount {
        &self.gas_fee_cap
    }
    fn gas_premium(&self) -> &TokenAmount {
        &self.gas_premium
    }
}

impl crate::message::MessageReadWrite for Message {
    fn set_gas_limit(&mut self, gas_limit: u64) {
        self.encoded.clear();
        self.gas_limit = gas_limit;
    }
    fn set_sequence(&mut self, sequence: u64) {
        self.encoded.clear();
        self.sequence = sequence;
    }
    fn set_gas_fee_cap(&mut self, gas_fee_cap: TokenAmount) {
        self.encoded.clear();
        self.gas_fee_cap = gas_fee_cap;
    }
    fn set_gas_premium(&mut self, gas_premium: TokenAmount) {
        self.encoded.clear();
        self.gas_premium = gas_premium;
    }
}

impl MessageBuilder {
    /// Every field has a default, so the generated `Result` cannot be an error.
    pub fn build(self) -> Message {
        let Ok(message) = self.build_infallible();
        message
    }
}

impl Message {
    pub fn builder() -> MessageBuilder {
        MessageBuilder::default()
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn from(&self) -> Address {
        self.from
    }

    pub fn to(&self) -> Address {
        self.to
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn method_num(&self) -> MethodNum {
        self.method_num
    }

    pub fn params(&self) -> &RawBytes {
        &self.params
    }

    pub fn gas_limit(&self) -> u64 {
        self.gas_limit
    }

    pub fn value(&self) -> &TokenAmount {
        &self.value
    }

    pub fn gas_fee_cap(&self) -> &TokenAmount {
        &self.gas_fee_cap
    }

    pub fn gas_premium(&self) -> &TokenAmount {
        &self.gas_premium
    }

    pub fn set_method_num(&mut self, method_num: MethodNum) {
        self.encoded.clear();
        self.method_num = method_num;
    }

    pub fn set_version(&mut self, version: u64) {
        self.encoded.clear();
        self.version = version;
    }

    pub fn set_from(&mut self, from: Address) {
        self.encoded.clear();
        self.from = from;
    }
}

macro_rules! message_conversion {
    ($($version:ty),+ $(,)?) => {
        $(
            impl From<$version> for Message {
                fn from(other: $version) -> Self {
                    Self {
                        version: other.version,
                        from: other.from.into(),
                        to: other.to.into(),
                        sequence: other.sequence,
                        value: other.value.into(),
                        method_num: other.method_num,
                        params: other.params,
                        gas_limit: other.gas_limit,
                        gas_fee_cap: other.gas_fee_cap.into(),
                        gas_premium: other.gas_premium.into(),
                        encoded: Memo::default(),
                    }
                }
            }

            impl From<Message> for $version {
                fn from(other: Message) -> Self {
                    Self {
                        version: other.version,
                        from: other.from.into(),
                        to: other.to.into(),
                        sequence: other.sequence,
                        value: other.value.into(),
                        method_num: other.method_num,
                        params: other.params,
                        gas_limit: other.gas_limit,
                        gas_fee_cap: other.gas_fee_cap.into(),
                        gas_premium: other.gas_premium.into(),
                    }
                }
            }

            impl From<&Message> for $version {
                fn from(other: &Message) -> Self {
                    other.clone().into()
                }
            }
        )+
    };
}

message_conversion!(Message_v3, Message_v4);

impl From<Message_v2> for Message {
    fn from(other: Message_v2) -> Self {
        Self {
            version: other.version as u64,
            from: other.from.into(),
            to: other.to.into(),
            sequence: other.sequence,
            value: other.value.into(),
            method_num: other.method_num,
            params: other.params,
            gas_limit: other.gas_limit as u64,
            gas_fee_cap: other.gas_fee_cap.into(),
            gas_premium: other.gas_premium.into(),
            encoded: Memo::default(),
        }
    }
}

/// Fallible because FVM2 cannot represent `f4` addresses, see [`Address::try_to_v2`].
impl TryFrom<Message> for Message_v2 {
    type Error = anyhow::Error;

    fn try_from(other: Message) -> Result<Self, Self::Error> {
        Ok(Self {
            version: other.version as i64,
            from: other.from.try_to_v2()?,
            to: other.to.try_to_v2()?,
            sequence: other.sequence,
            value: other.value.into(),
            method_num: other.method_num,
            params: other.params,
            gas_limit: other.gas_limit as i64,
            gas_fee_cap: other.gas_fee_cap.into(),
            gas_premium: other.gas_premium.into(),
        })
    }
}

impl TryFrom<&Message> for Message_v2 {
    type Error = anyhow::Error;

    fn try_from(other: &Message) -> Result<Self, Self::Error> {
        other.clone().try_into()
    }
}

impl Message {
    /// Does some basic checks on the Message to see if the fields are valid.
    pub fn check(self: &Message) -> anyhow::Result<()> {
        if self.gas_limit == 0 {
            return Err(anyhow!("Message has no gas limit set"));
        }
        if self.gas_limit > i64::MAX as u64 {
            return Err(anyhow!("Message gas exceeds i64 max"));
        }
        Ok(())
    }

    /// Creates a new Message to transfer an amount of FIL specified in the `value` field.
    pub fn transfer(from: Address, to: Address, value: TokenAmount) -> Self {
        Message {
            from,
            to,
            value,
            method_num: METHOD_SEND,
            ..Default::default()
        }
    }

    pub fn cid(&self) -> cid::Cid {
        self.encoded().cid()
    }

    /// The encoded length, which the on-chain gas floor is charged over.
    pub fn encoded_len(&self) -> usize {
        self.encoded().byte_len()
    }

    fn encoded(&self) -> &EncodedCbor {
        self.encoded.get_or_init(|| {
            EncodedCbor::compute(self).expect("message serialization is infallible")
        })
    }

    /// Tests if a message is equivalent to another replacing message.
    /// A replacing message is a message with a different CID,
    /// any of Gas values, and different signature, but with all
    /// other parameters matching (source/destination, nonce, parameters, etc.)
    /// See <https://github.com/filecoin-project/lotus/blob/813d133c24295629ef442fc3aa60e6e6b2101226/chain/types/message.go#L138>
    pub fn equal_call(&self, other: &Self) -> bool {
        self.version == other.version
            && self.from == other.from
            && self.to == other.to
            && self.sequence == other.sequence
            && self.value == other.value
            && self.method_num == other.method_num
            && self.params == other.params
    }
}

impl Serialize for Message {
    fn serialize<S>(&self, s: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.version,
            &self.to,
            &self.from,
            &self.sequence,
            &self.value,
            &self.gas_limit,
            &self.gas_fee_cap,
            &self.gas_premium,
            &self.method_num,
            &self.params,
        )
            .serialize(s)
    }
}

impl<'de> Deserialize<'de> for Message {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (
            version,
            to,
            from,
            sequence,
            value,
            gas_limit,
            gas_fee_cap,
            gas_premium,
            method_num,
            params,
        ) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            encoded: Memo::default(),
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
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{MessageRead as _, MessageReadWrite as _};
    use crate::utils::cid::CidCborExt as _;
    use fvm_ipld_encoding::to_vec;
    use quickcheck_macros::quickcheck;

    fn hash_of<T: std::hash::Hash>(value: &T) -> u64 {
        use std::hash::Hasher as _;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    /// `Arbitrary` always yields a cold memo, so warming one here is the only way to reach the
    /// case the derived `PartialEq`/`Hash`/`Debug` must stay blind to.
    #[quickcheck]
    fn computing_the_memo_is_invisible(msg: Message) -> bool {
        let warm = msg.clone();
        let _ = warm.cid();
        warm == msg && hash_of(&warm) == hash_of(&msg) && format!("{warm:?}") == format!("{msg:?}")
    }

    #[quickcheck]
    fn cid_and_len_match_the_unmemoized_path(msg: Message) -> bool {
        msg.cid() == cid::Cid::from_cbor_blake2b256(&msg).unwrap()
            && msg.encoded_len() == to_vec(&msg).unwrap().len()
    }

    #[track_caller]
    fn assert_invalidates(field: &str, mutate: impl FnOnce(&mut Message)) {
        let mut msg = Message::builder()
            .to(Address::new_id(1))
            .from(Address::new_id(2))
            .build();
        let before = msg.cid();
        mutate(&mut msg);
        assert_ne!(msg.cid(), before, "set_{field} left the CID unchanged");
        assert_eq!(msg.cid(), cid::Cid::from_cbor_blake2b256(&msg).unwrap());
        assert_eq!(
            msg.chain_length().unwrap(),
            to_vec(&msg).unwrap().len(),
            "set_{field} left a stale length"
        );
    }

    /// Both the CID and the chain length are derived from the fields, so no setter may leave
    /// either describing the bytes the message used to encode to.
    #[test]
    fn every_setter_invalidates_both_derived_values() {
        assert_invalidates("version", |m| m.set_version(1));
        assert_invalidates("from", |m| m.set_from(Address::new_id(99)));
        assert_invalidates("method_num", |m| m.set_method_num(7));
        assert_invalidates("sequence", |m| m.set_sequence(5));
        assert_invalidates("gas_limit", |m| m.set_gas_limit(1234));
        assert_invalidates("gas_fee_cap", |m| {
            m.set_gas_fee_cap(TokenAmount::from_atto(11))
        });
        assert_invalidates("gas_premium", |m| {
            m.set_gas_premium(TokenAmount::from_atto(13))
        });
    }

    #[quickcheck]
    fn message_v4_roundtrip(msg: Message) {
        let round_tripped: Message = Message_v4::from(msg.clone()).into();
        assert_eq!(round_tripped, msg);
    }

    #[quickcheck]
    fn message_v3_roundtrip(msg: Message) {
        let round_tripped: Message = Message_v3::from(msg.clone()).into();
        assert_eq!(round_tripped, msg);
    }

    #[quickcheck]
    fn message_v2_roundtrip(msg: Message) {
        use crate::shim::address::Protocol;

        let representable = msg.from().protocol() != Protocol::Delegated
            && msg.to().protocol() != Protocol::Delegated;
        match Message_v2::try_from(msg.clone()) {
            Ok(v2) => {
                assert!(representable);
                let round_tripped: Message = v2.into();
                assert_eq!(round_tripped, msg);
            }
            Err(_) => assert!(!representable),
        }
    }
}
