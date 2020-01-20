// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use super::Message;
use crate::TokenAmount;
use crate::{MethodNum, Serialized};

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
    params: Serialized,
    #[builder(default)]
    gas_price: BigUint,
    #[builder(default)]
    gas_limit: BigUint,
}

// TODO verify format or implement custom serialize/deserialize function (if necessary):
// https://github.com/ChainSafe/ferret/issues/143

impl UnsignedMessage {
    pub fn builder() -> MessageBuilder {
        MessageBuilder::default()
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
}

impl Cbor for UnsignedMessage {}
