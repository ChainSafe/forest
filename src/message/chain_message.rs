// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::message::MethodNum;
use crate::shim::{address::Address, econ::TokenAmount, message::Message};
use fvm_ipld_encoding::RawBytes;
use serde::{Deserialize, Serialize};

use super::Message as MessageTrait;
use crate::message::signed_message::SignedMessage;

/// `Enum` to encapsulate signed and unsigned messages. Useful when working with
/// both types
#[derive(Clone, Debug, Serialize, Deserialize, Hash)]
#[serde(untagged)]
pub enum ChainMessage {
    Unsigned(Message),
    Signed(SignedMessage),
}

impl ChainMessage {
    pub fn message(&self) -> &Message {
        match self {
            Self::Unsigned(m) => m,
            Self::Signed(sm) => sm.message(),
        }
    }

    pub fn cid(&self) -> cid::Cid {
        match self {
            ChainMessage::Unsigned(msg) => msg.cid(),
            ChainMessage::Signed(msg) => msg.cid(),
        }
    }

    /// Tests if a message is equivalent to another replacing message.
    /// A replacing message is a message with a different CID,
    /// any of Gas values, and different signature, but with all
    /// other parameters matching (source/destination, nonce, parameters, etc.)
    /// See <https://github.com/filecoin-project/lotus/blob/813d133c24295629ef442fc3aa60e6e6b2101226/chain/types/message.go#L138>
    pub fn equal_call(&self, other: &Self) -> bool {
        self.message().equal_call(other.message())
    }
}

impl MessageTrait for ChainMessage {
    fn from(&self) -> Address {
        match self {
            Self::Signed(t) => t.from(),
            Self::Unsigned(t) => t.from,
        }
    }
    fn to(&self) -> Address {
        match self {
            Self::Signed(t) => t.to(),
            Self::Unsigned(t) => t.to,
        }
    }
    fn sequence(&self) -> u64 {
        match self {
            Self::Signed(t) => t.sequence(),
            Self::Unsigned(t) => t.sequence,
        }
    }
    fn value(&self) -> TokenAmount {
        match self {
            Self::Signed(t) => t.value(),
            Self::Unsigned(t) => t.value.clone(),
        }
    }
    fn method_num(&self) -> MethodNum {
        match self {
            Self::Signed(t) => t.method_num(),
            Self::Unsigned(t) => t.method_num,
        }
    }
    fn params(&self) -> &RawBytes {
        match self {
            Self::Signed(t) => t.params(),
            Self::Unsigned(t) => t.params(),
        }
    }
    fn gas_limit(&self) -> u64 {
        match self {
            Self::Signed(t) => t.gas_limit(),
            Self::Unsigned(t) => t.gas_limit(),
        }
    }
    fn set_gas_limit(&mut self, token_amount: u64) {
        match self {
            Self::Signed(t) => t.set_gas_limit(token_amount),
            Self::Unsigned(t) => t.set_gas_limit(token_amount),
        }
    }
    fn set_sequence(&mut self, new_sequence: u64) {
        match self {
            Self::Signed(t) => t.set_sequence(new_sequence),
            Self::Unsigned(t) => t.set_sequence(new_sequence),
        }
    }
    fn required_funds(&self) -> TokenAmount {
        match self {
            Self::Signed(t) => t.required_funds(),
            Self::Unsigned(t) => &t.gas_fee_cap * t.gas_limit + &t.value,
        }
    }
    fn gas_fee_cap(&self) -> TokenAmount {
        match self {
            Self::Signed(t) => t.gas_fee_cap(),
            Self::Unsigned(t) => t.gas_fee_cap.clone(),
        }
    }
    fn gas_premium(&self) -> TokenAmount {
        match self {
            Self::Signed(t) => t.gas_premium(),
            Self::Unsigned(t) => t.gas_premium.clone(),
        }
    }

    fn set_gas_fee_cap(&mut self, cap: TokenAmount) {
        match self {
            Self::Signed(t) => t.set_gas_fee_cap(cap),
            Self::Unsigned(t) => t.set_gas_fee_cap(cap),
        }
    }

    fn set_gas_premium(&mut self, prem: TokenAmount) {
        match self {
            Self::Signed(t) => t.set_gas_premium(prem),
            Self::Unsigned(t) => t.set_gas_premium(prem),
        }
    }
}

impl From<Message> for ChainMessage {
    fn from(value: Message) -> Self {
        Self::Unsigned(value)
    }
}

impl From<SignedMessage> for ChainMessage {
    fn from(value: SignedMessage) -> Self {
        Self::Signed(value)
    }
}
