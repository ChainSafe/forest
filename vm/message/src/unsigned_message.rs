// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use super::Message;
use crate::TokenAmount;
use crate::{MethodNum, MethodParams};

use address::Address;
use derive_builder::Builder;
use encoding::Cbor;
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};

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
#[derive(PartialEq, Clone, Debug, Builder, Serialize, Deserialize)]
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

impl Message for UnsignedMessage {
    /// from returns the from address of the message
    fn from(&self) -> Address {
        self.from.clone()
    }
    /// to returns the destination address of the message
    fn to(&self) -> Address {
        self.to.clone()
    }
    /// sequence returns the message sequence or nonce
    fn sequence(&self) -> u64 {
        self.sequence
    }
    /// value returns the amount sent in message
    fn value(&self) -> TokenAmount {
        self.value.clone()
    }
    /// method_num returns the method number to be called
    fn method_num(&self) -> MethodNum {
        self.method_num.clone()
    }
    /// params returns the encoded parameters for the method call
    fn params(&self) -> MethodParams {
        self.params.clone()
    }
    /// gas_price returns gas price for the message
    fn gas_price(&self) -> BigUint {
        self.gas_price.clone()
    }
    /// gas_limit returns the gas limit for the message
    fn gas_limit(&self) -> BigUint {
        self.gas_limit.clone()
    }
}

// TODO modify unsigned message encoding format when needed
impl Cbor for UnsignedMessage {}
