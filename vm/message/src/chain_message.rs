// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Message;
use crate::signed_message::SignedMessage;
use crate::unsigned_message::UnsignedMessage;
use address::Address;
use cid::Cid;
use encoding::{Cbor, Error};
use serde::{Deserialize, Serialize};
use vm::{MethodNum, Serialized, TokenAmount};

/// Enum to encpasulate signed and unsigned messages. Useful when working with both types
#[derive(Clone, Debug, Serialize, Deserialize, Hash)]
#[serde(untagged)]
pub enum ChainMessage {
    Unsigned(UnsignedMessage),
    Signed(SignedMessage),
}

impl ChainMessage {
    pub fn message(&self) -> &UnsignedMessage {
        match self {
            Self::Unsigned(m) => &m,
            Self::Signed(sm) => &sm.message(),
        }
    }
}

impl Message for ChainMessage {
    fn from(&self) -> &Address {
        match self {
            Self::Signed(t) => t.from(),
            Self::Unsigned(t) => t.from(),
        }
    }
    fn to(&self) -> &Address {
        match self {
            Self::Signed(t) => t.to(),
            Self::Unsigned(t) => t.to(),
        }
    }
    fn sequence(&self) -> u64 {
        match self {
            Self::Signed(t) => t.sequence(),
            Self::Unsigned(t) => t.sequence(),
        }
    }
    fn value(&self) -> &TokenAmount {
        match self {
            Self::Signed(t) => t.value(),
            Self::Unsigned(t) => t.value(),
        }
    }
    fn method_num(&self) -> MethodNum {
        match self {
            Self::Signed(t) => t.method_num(),
            Self::Unsigned(t) => t.method_num(),
        }
    }
    fn params(&self) -> &Serialized {
        match self {
            Self::Signed(t) => t.params(),
            Self::Unsigned(t) => t.params(),
        }
    }
    fn gas_limit(&self) -> i64 {
        match self {
            Self::Signed(t) => t.gas_limit(),
            Self::Unsigned(t) => t.gas_limit(),
        }
    }
    fn set_gas_limit(&mut self, token_amount: i64) {
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
            Self::Unsigned(t) => t.required_funds(),
        }
    }
    fn gas_fee_cap(&self) -> &TokenAmount {
        match self {
            Self::Signed(t) => t.gas_fee_cap(),
            Self::Unsigned(t) => t.gas_fee_cap(),
        }
    }
    fn gas_premium(&self) -> &TokenAmount {
        match self {
            Self::Signed(t) => t.gas_premium(),
            Self::Unsigned(t) => t.gas_premium(),
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

impl Cbor for ChainMessage {
    /// Returns the content identifier of the raw block of data
    /// Default is Blake2b256 hash
    fn cid(&self) -> Result<Cid, Error> {
        match self {
            Self::Signed(t) => t.cid(),
            Self::Unsigned(t) => t.cid(),
        }
    }
}
