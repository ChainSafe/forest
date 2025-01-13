// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use anyhow::anyhow;

use fvm_ipld_encoding::de::Deserializer;
use fvm_ipld_encoding::ser::Serializer;
use fvm_ipld_encoding::RawBytes;
use fvm_shared2::message::Message as Message_v2;
pub use fvm_shared3::message::Message as Message_v3;
pub use fvm_shared3::METHOD_SEND;
use fvm_shared4::message::Message as Message_v4;
use serde::{Deserialize, Serialize};

use crate::shim::{address::Address, econ::TokenAmount};

/// Method number indicator for calling actor methods.
pub type MethodNum = u64;

#[derive(Clone, Default, PartialEq, Eq, Debug, Hash)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub struct Message {
    pub version: u64,
    pub from: Address,
    pub to: Address,
    pub sequence: u64,
    pub value: TokenAmount,
    pub method_num: MethodNum,
    #[cfg_attr(test, arbitrary(gen(
        |g| RawBytes::new(Vec::arbitrary(g))
    )))]
    pub params: RawBytes,
    pub gas_limit: u64,
    pub gas_fee_cap: TokenAmount,
    pub gas_premium: TokenAmount,
}

impl From<Message_v4> for Message {
    fn from(other: Message_v4) -> Self {
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

impl From<Message> for Message_v4 {
    fn from(other: Message) -> Self {
        (&other).into()
    }
}

impl From<&Message> for Message_v4 {
    fn from(other: &Message) -> Self {
        let other: Message = other.clone();
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

impl From<Message_v3> for Message {
    fn from(other: Message_v3) -> Self {
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

impl From<Message> for Message_v3 {
    fn from(other: Message) -> Self {
        (&other).into()
    }
}

impl From<&Message> for Message_v3 {
    fn from(other: &Message) -> Self {
        let other: Message = other.clone();
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
        }
    }
}

impl From<Message> for Message_v2 {
    fn from(other: Message) -> Self {
        Self {
            version: other.version as i64,
            from: other.from.into(),
            to: other.to.into(),
            sequence: other.sequence,
            value: other.value.into(),
            method_num: other.method_num,
            params: other.params,
            gas_limit: other.gas_limit as i64,
            gas_fee_cap: other.gas_fee_cap.into(),
            gas_premium: other.gas_premium.into(),
        }
    }
}

impl From<&Message> for Message_v2 {
    fn from(other: &Message) -> Self {
        other.clone().into()
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
        use crate::utils::cid::CidCborExt;
        cid::Cid::from_cbor_blake2b256(self).expect("message serialization is infallible")
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
