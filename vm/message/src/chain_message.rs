// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Message as MessageTrait;
use crate::signed_message::SignedMessage;

use cid::{multihash, Cid};
use fvm_ipld_encoding::{Cbor, Error, RawBytes, DAG_CBOR};
use fvm_shared::message::Message;
use fvm_shared::MethodNum;
use fvm_shared::{address::Address, econ::TokenAmount};
use serde::{Deserialize, Serialize};

impl CidHash for Message {}
impl CidHash for SignedMessage {}

pub trait CidHash: fvm_ipld_encoding::Cbor {
    fn cid(&self) -> Result<Cid, Error> {
        use multihash::MultihashDigest;
        const DIGEST_SIZE: u32 = 32; // TODO get from the multihash?
        let data = &self.marshal_cbor()?;
        let hash = multihash::Code::Blake2b256.digest(data);
        debug_assert_eq!(
            u32::from(hash.size()),
            DIGEST_SIZE,
            "expected 32byte digest"
        );
        Ok(Cid::new_v1(DAG_CBOR, hash))
    }
}

/// `Enum` to encapsulate signed and unsigned messages. Useful when working with both types
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
}

impl MessageTrait for ChainMessage {
    fn from(&self) -> &Address {
        match self {
            Self::Signed(t) => t.from(),
            Self::Unsigned(t) => &t.from,
        }
    }
    fn to(&self) -> &Address {
        match self {
            Self::Signed(t) => t.to(),
            Self::Unsigned(t) => &t.to,
        }
    }
    fn sequence(&self) -> u64 {
        match self {
            Self::Signed(t) => t.sequence(),
            Self::Unsigned(t) => t.sequence,
        }
    }
    fn value(&self) -> &TokenAmount {
        match self {
            Self::Signed(t) => t.value(),
            Self::Unsigned(t) => &t.value,
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
            Self::Unsigned(t) => &t.params,
        }
    }
    fn gas_limit(&self) -> i64 {
        match self {
            Self::Signed(t) => t.gas_limit(),
            Self::Unsigned(t) => t.gas_limit,
        }
    }
    fn set_gas_limit(&mut self, token_amount: i64) {
        match self {
            Self::Signed(t) => t.set_gas_limit(token_amount),
            Self::Unsigned(t) => t.gas_limit = token_amount,
        }
    }
    fn set_sequence(&mut self, new_sequence: u64) {
        match self {
            Self::Signed(t) => t.set_sequence(new_sequence),
            Self::Unsigned(t) => t.sequence = new_sequence,
        }
    }
    fn required_funds(&self) -> TokenAmount {
        match self {
            Self::Signed(t) => t.required_funds(),
            Self::Unsigned(t) => &t.gas_fee_cap * t.gas_limit + &t.value,
        }
    }
    fn gas_fee_cap(&self) -> &TokenAmount {
        match self {
            Self::Signed(t) => t.gas_fee_cap(),
            Self::Unsigned(t) => &t.gas_fee_cap,
        }
    }
    fn gas_premium(&self) -> &TokenAmount {
        match self {
            Self::Signed(t) => t.gas_premium(),
            Self::Unsigned(t) => &t.gas_premium,
        }
    }

    fn set_gas_fee_cap(&mut self, cap: TokenAmount) {
        match self {
            Self::Signed(t) => t.set_gas_fee_cap(cap),
            Self::Unsigned(t) => t.gas_fee_cap = cap,
        }
    }

    fn set_gas_premium(&mut self, prem: TokenAmount) {
        match self {
            Self::Signed(t) => t.set_gas_premium(prem),
            Self::Unsigned(t) => t.gas_premium = prem,
        }
    }
}

impl ChainMessage {
    /// Returns the content identifier of the raw block of data
    /// Default is `Blake2b256` hash
    fn cid(&self) -> Result<Cid, Error> {
        match self {
            Self::Signed(t) => t.cid(),
            Self::Unsigned(t) => t.cid(),
        }
    }
}
