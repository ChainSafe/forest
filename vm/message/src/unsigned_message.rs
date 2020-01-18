// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use super::Message;
use address::Address;
use derive_builder::Builder;
use encoding::{de, ser, Cbor};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use vm::{MethodNum, MethodParams, TokenAmount};

/// Default Unsigned VM message type which includes all data needed for a state transition
///
/// Usage:
/// ```
/// use message::{UnsignedMessage, Message};
/// use vm::{TokenAmount, MethodParams, MethodNum};
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
///     .params(MethodParams::default()) // optional
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
    params: MethodParams,
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

/// Structure defines how the fields are cbor encoded as an unsigned message
#[derive(Serialize, Deserialize)]
struct CborUnsignedMessage(
    Address,      // To
    Address,      // from
    u64,          // Sequence
    TokenAmount,  // Value
    BigUint,      // GasPrice
    BigUint,      // GasLimit
    MethodNum,    // Method
    MethodParams, // Params
);

impl ser::Serialize for UnsignedMessage {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let value: CborUnsignedMessage = CborUnsignedMessage(
            self.to.clone(),
            self.from.clone(),
            self.sequence,
            self.value.clone(),
            self.gas_price.clone(),
            self.gas_limit.clone(),
            self.method_num,
            self.params.clone(),
        );
        CborUnsignedMessage::serialize(&value, s)
    }
}

impl<'de> de::Deserialize<'de> for UnsignedMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let cm = CborUnsignedMessage::deserialize(deserializer)?;
        Ok(Self {
            to: cm.0,
            from: cm.1,
            sequence: cm.2,
            value: cm.3,
            gas_price: cm.4,
            gas_limit: cm.5,
            method_num: cm.6,
            params: cm.7,
        })
    }
}

impl Message for UnsignedMessage {
    /// from returns the from address of the message
    fn from(&self) -> &Address {
        &self.from
    }
    /// to returns the destination address of the message
    fn to(&self) -> &Address {
        &self.to
    }
    /// sequence returns the message sequence or nonce
    fn sequence(&self) -> u64 {
        self.sequence
    }
    /// value returns the amount sent in message
    fn value(&self) -> &TokenAmount {
        &self.value
    }
    /// method_num returns the method number to be called
    fn method_num(&self) -> &MethodNum {
        &self.method_num
    }
    /// params returns the encoded parameters for the method call
    fn params(&self) -> &MethodParams {
        &self.params
    }
    /// gas_price returns gas price for the message
    fn gas_price(&self) -> &BigUint {
        &self.gas_price
    }
    /// gas_limit returns the gas limit for the message
    fn gas_limit(&self) -> &BigUint {
        &self.gas_limit
    }
}

// TODO modify unsigned message encoding format when needed
impl Cbor for UnsignedMessage {}
