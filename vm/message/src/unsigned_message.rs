// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Message;
use address::Address;
use derive_builder::Builder;
use encoding::{de, ser, Cbor};
use num_bigint::biguint_ser::{BigUintDe, BigUintSer};
use serde::Deserialize;
use vm::{MethodNum, Serialized, TokenAmount};

/// Default Unsigned VM message type which includes all data needed for a state transition
///
/// Usage:
/// ```
/// use forest_message::{UnsignedMessage, Message};
/// use vm::{TokenAmount, Serialized, MethodNum};
/// use address::Address;
///
/// // Use the builder pattern to generate a message
/// let message = UnsignedMessage::builder()
///     .to(Address::new_id(0).unwrap())
///     .from(Address::new_id(1).unwrap())
///     .sequence(0) // optional
///     .value(TokenAmount::from(0u8)) // optional
///     .method_num(MethodNum::default()) // optional
///     .params(Serialized::default()) // optional
///     .gas_limit(0) // optional
///     .gas_price(TokenAmount::from(0u8)) // optional
///     .build()
///     .unwrap();
///
/// // Commands can be chained, or built seperately
/// let mut message_builder = UnsignedMessage::builder();
/// message_builder.sequence(1);
/// message_builder.from(Address::new_id(0).unwrap());
/// message_builder.to(Address::new_id(1).unwrap());
/// let msg = message_builder.build().unwrap();
/// assert_eq!(msg.sequence(), 1);
/// ```
#[derive(PartialEq, Clone, Debug, Builder)]
#[builder(name = "MessageBuilder")]
pub struct UnsignedMessage {
    from: Address,
    to: Address,
    #[builder(default)]
    sequence: u64,
    #[builder(default)]
    value: TokenAmount,
    #[builder(default)]
    method_num: MethodNum,
    #[builder(default)]
    params: Serialized,
    #[builder(default)]
    gas_price: TokenAmount,
    #[builder(default)]
    gas_limit: u64,
}

impl UnsignedMessage {
    pub fn builder() -> MessageBuilder {
        MessageBuilder::default()
    }
}

impl ser::Serialize for UnsignedMessage {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        (
            &self.to,
            &self.from,
            &self.sequence,
            BigUintSer(&self.value),
            BigUintSer(&self.gas_price),
            &self.gas_limit,
            &self.method_num,
            &self.params,
        )
            .serialize(s)
    }
}

impl<'de> de::Deserialize<'de> for UnsignedMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let (
            to,
            from,
            sequence,
            BigUintDe(value),
            BigUintDe(gas_price),
            gas_limit,
            method_num,
            params,
        ) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            to,
            from,
            sequence,
            value,
            gas_price,
            gas_limit,
            method_num,
            params,
        })
    }
}

impl Message for UnsignedMessage {
    fn from(&self) -> &Address {
        &self.from
    }
    fn to(&self) -> &Address {
        &self.to
    }
    fn sequence(&self) -> u64 {
        self.sequence
    }
    fn value(&self) -> &TokenAmount {
        &self.value
    }
    fn method_num(&self) -> MethodNum {
        self.method_num
    }
    fn params(&self) -> &Serialized {
        &self.params
    }
    fn gas_price(&self) -> &TokenAmount {
        &self.gas_price
    }
    fn gas_limit(&self) -> u64 {
        self.gas_limit
    }
    fn required_funds(&self) -> TokenAmount {
        let total: TokenAmount = self.gas_price() * self.gas_limit();
        total + self.value()
    }
}

impl Cbor for UnsignedMessage {}
