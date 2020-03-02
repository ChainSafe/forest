// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Message;
use address::Address;
use derive_builder::Builder;
use encoding::{de, ser, Cbor};
use num_bigint::{biguint_ser, BigUint};
use serde::{Deserialize, Serialize};
use vm::{MethodNum, Serialized, TokenAmount};

/// Default Unsigned VM message type which includes all data needed for a state transition
///
/// Usage:
/// ```
/// use forest_message::{UnsignedMessage, Message};
/// use vm::{TokenAmount, Serialized, MethodNum};
/// use num_bigint::BigUint;
/// use address::Address;
///
/// // Use the builder pattern to generate a message
/// let message = UnsignedMessage::builder()
///     .to(Address::new_id(0).unwrap())
///     .from(Address::new_id(1).unwrap())
///     .sequence(0) // optional
///     .value(TokenAmount::new(0)) // optional
///     .method_num(MethodNum::default()) // optional
///     .params(Serialized::default()) // optional
///     .gas_limit(BigUint::default()) // optional
///     .gas_price(BigUint::default()) // optional
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
    gas_price: BigUint,
    #[builder(default)]
    gas_limit: BigUint,
}

impl UnsignedMessage {
    pub fn builder() -> MessageBuilder {
        MessageBuilder::default()
    }
}

// Type declared outside of deserialize block because of clippy bug
#[derive(Deserialize)]
pub struct TupleUnsignedMessage(
    Address,
    Address,
    u64,
    TokenAmount,
    #[serde(with = "biguint_ser")] BigUint,
    #[serde(with = "biguint_ser")] BigUint,
    MethodNum,
    Serialized,
);

impl ser::Serialize for UnsignedMessage {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        #[derive(Serialize)]
        pub struct TupleUnsignedMessage<'a>(
            &'a Address,
            &'a Address,
            &'a u64,
            &'a TokenAmount,
            #[serde(with = "biguint_ser")] &'a BigUint,
            #[serde(with = "biguint_ser")] &'a BigUint,
            &'a MethodNum,
            &'a Serialized,
        );
        TupleUnsignedMessage(
            &self.to,
            &self.from,
            &self.sequence,
            &self.value,
            &self.gas_price,
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
        let TupleUnsignedMessage(
            to,
            from,
            sequence,
            value,
            gas_price,
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
    fn method_num(&self) -> &MethodNum {
        &self.method_num
    }
    fn params(&self) -> &Serialized {
        &self.params
    }
    fn gas_price(&self) -> &BigUint {
        &self.gas_price
    }
    fn gas_limit(&self) -> &BigUint {
        &self.gas_limit
    }
    fn required_funds(&self) -> BigUint {
        let total = self.gas_price() * self.gas_limit();
        total + self.value().0.clone()
    }
}

impl Cbor for UnsignedMessage {}
