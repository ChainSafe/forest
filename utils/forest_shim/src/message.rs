// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::ops::{Deref, DerefMut};

use fvm_ipld_encoding::{Cbor, RawBytes as RawBytes_v2};
use fvm_ipld_encoding3::RawBytes as RawBytes_v3;
use fvm_shared::message::Message as Message_v2;
pub use fvm_shared3::message::Message as Message_v3;
use serde::{Deserialize, Serialize};

use crate::{address::Address, econ::TokenAmount};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug, Hash)]
#[serde(transparent)]
pub struct Message(Message_v3);

impl Deref for Message {
    type Target = Message_v3;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Message {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<Message_v3> for Message {
    fn from(other: Message_v3) -> Self {
        Message(other)
    }
}

impl From<Message> for Message_v3 {
    fn from(other: Message) -> Self {
        other.0
    }
}

impl From<&Message> for Message_v3 {
    fn from(other: &Message) -> Self {
        other.0.clone()
    }
}

impl From<Message_v2> for Message {
    fn from(other: Message_v2) -> Self {
        Message(Message_v3 {
            version: other.version as u64,
            from: Address::from(other.from).into(),
            to: Address::from(other.to).into(),
            sequence: other.sequence,
            value: TokenAmount::from(other.value).into(),
            method_num: other.method_num,
            params: RawBytes_v3::from(other.params.to_vec()),
            gas_limit: other.gas_limit as u64,
            gas_fee_cap: TokenAmount::from(other.gas_fee_cap).into(),
            gas_premium: TokenAmount::from(other.gas_premium).into(),
        })
    }
}

impl From<Message> for Message_v2 {
    fn from(other: Message) -> Self {
        let other: Message_v3 = other.0;
        Message_v2 {
            version: other.version as i64,
            from: Address::from(other.from).into(),
            to: Address::from(other.to).into(),
            sequence: other.sequence,
            value: TokenAmount::from(other.value).into(),
            method_num: other.method_num,
            params: RawBytes_v2::from(other.params.to_vec()),
            gas_limit: other.gas_limit as i64,
            gas_fee_cap: TokenAmount::from(other.gas_fee_cap).into(),
            gas_premium: TokenAmount::from(other.gas_premium).into(),
        }
    }
}

impl From<&Message> for Message_v2 {
    fn from(other: &Message) -> Self {
        let other: Message_v3 = other.0.clone();
        Message_v2 {
            version: other.version as i64,
            from: Address::from(other.from).into(),
            to: Address::from(other.to).into(),
            sequence: other.sequence,
            value: TokenAmount::from(other.value).into(),
            method_num: other.method_num,
            params: RawBytes_v2::from(Vec::<u8>::from(other.params)),
            gas_limit: other.gas_limit as i64,
            gas_fee_cap: TokenAmount::from(other.gas_fee_cap).into(),
            gas_premium: TokenAmount::from(other.gas_premium).into(),
        }
    }
}

impl Cbor for Message {}
