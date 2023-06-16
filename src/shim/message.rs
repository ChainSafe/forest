// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use anyhow::anyhow;
use std::ops::{Deref, DerefMut};

use fvm_ipld_encoding::{Cbor, RawBytes as RawBytes_v2};
use fvm_ipld_encoding3::RawBytes as RawBytes_v3;
use fvm_shared::message::Message as Message_v2;
pub use fvm_shared3::message::Message as Message_v3;
use fvm_shared3::{MethodNum, METHOD_SEND};
use serde::{Deserialize, Serialize};

use crate::shim::{address::Address, econ::TokenAmount};

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize, Debug, Hash)]
pub struct Message {
    pub version: u64,
    pub from: Address,
    pub to: Address,
    pub sequence: u64,
    pub value: TokenAmount,
    pub method_num: MethodNum,
    pub params: RawBytes_v3,
    pub gas_limit: u64,
    pub gas_fee_cap: TokenAmount,
    pub gas_premium: TokenAmount,
}

impl quickcheck::Arbitrary for Message {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Self {
            to: Address::arbitrary(g),
            from: Address::arbitrary(g),
            version: u64::arbitrary(g),
            sequence: u64::arbitrary(g),
            value: TokenAmount::arbitrary(g),
            method_num: u64::arbitrary(g),
            params: fvm_ipld_encoding3::RawBytes::new(Vec::arbitrary(g)),
            gas_limit: u64::arbitrary(g),
            gas_fee_cap: TokenAmount::arbitrary(g),
            gas_premium: TokenAmount::arbitrary(g),
        }
    }
}

impl From<Message_v3> for Message {
    fn from(other: Message_v3) -> Self {
        Message {
            version: other.version,
            from: other.from.into(),
            to: other.to.into(),
            sequence: other.sequence,
            value: other.value.into(),
            method_num: other.method_num,
            params: RawBytes_v3::from(other.params.to_vec()),
            gas_limit: other.gas_limit,
            gas_fee_cap: other.gas_fee_cap.into(),
            gas_premium: other.gas_premium.into(),
        }
    }
}

impl From<Message> for Message_v3 {
    fn from(other: Message) -> Self {
        Message_v3 {
            version: other.version,
            from: other.from.into(),
            to: other.to.into(),
            sequence: other.sequence,
            value: other.value.into(),
            method_num: other.method_num,
            params: RawBytes_v3::from(other.params.to_vec()),
            gas_limit: other.gas_limit,
            gas_fee_cap: other.gas_fee_cap.into(),
            gas_premium: other.gas_premium.into(),
        }
    }
}

impl From<&Message> for Message_v3 {
    fn from(other: &Message) -> Self {
        let other: Message = other.clone();
        Message_v3 {
            version: other.version,
            from: other.from.into(),
            to: other.to.into(),
            sequence: other.sequence,
            value: other.value.into(),
            method_num: other.method_num,
            params: RawBytes_v3::from(other.params.to_vec()),
            gas_limit: other.gas_limit,
            gas_fee_cap: other.gas_fee_cap.into(),
            gas_premium: other.gas_premium.into(),
        }
    }
}

impl From<Message_v2> for Message {
    fn from(other: Message_v2) -> Self {
        Message {
            version: other.version as u64,
            from: other.from.into(),
            to: other.to.into(),
            sequence: other.sequence,
            value: other.value.into(),
            method_num: other.method_num,
            params: RawBytes_v3::from(other.params.to_vec()),
            gas_limit: other.gas_limit as u64,
            gas_fee_cap: other.gas_fee_cap.into(),
            gas_premium: other.gas_premium.into(),
        }
    }
}

impl From<Message> for Message_v2 {
    fn from(other: Message) -> Self {
        Message_v2 {
            version: other.version as i64,
            from: other.from.into(),
            to: other.to.into(),
            sequence: other.sequence,
            value: other.value.into(),
            method_num: other.method_num,
            params: RawBytes_v2::from(other.params.to_vec()),
            gas_limit: other.gas_limit as i64,
            gas_fee_cap: other.gas_fee_cap.into(),
            gas_premium: other.gas_premium.into(),
        }
    }
}

impl From<&Message> for Message_v2 {
    fn from(other: &Message) -> Self {
        let other: Message = other.clone();
        Message_v2 {
            version: other.version as i64,
            from: other.from.into(),
            to: other.to.into(),
            sequence: other.sequence,
            value: other.value.into(),
            method_num: other.method_num,
            params: RawBytes_v2::from(other.params.to_vec()),
            gas_limit: other.gas_limit as i64,
            gas_fee_cap: other.gas_fee_cap.into(),
            gas_premium: other.gas_premium.into(),
        }
    }
}

impl Cbor for Message {}

impl Message {
    pub fn check(self: &Message) -> anyhow::Result<()> {
        if self.gas_limit == 0 {
            return Err(anyhow!("Message has no gas limit set"));
        }
        if self.gas_limit > i64::MAX as u64 {
            return Err(anyhow!("Message gas exceeds i64 max"));
        }
        Ok(())
    }

    pub fn transfer(from: Address, to: Address, value: TokenAmount) -> Self {
        Message {
            from,
            to,
            value,
            method_num: METHOD_SEND,
            ..Default::default()
        }
    }
}
