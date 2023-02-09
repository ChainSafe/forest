// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::ops::{Deref, DerefMut};

use cid::Cid;
use fil_actors_runtime::cbor::serialize;
use fvm_ipld_encoding::DAG_CBOR;
use fvm_shared::message::Message as MessageV2;
use fvm_shared3::message::Message as MessageV3;
use serde::{Deserialize, Serialize};

use crate::{address::Address, econ::TokenAmount};

#[derive(PartialEq, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Message(MessageV3);

impl Message {
    pub fn cid(&self) -> Cid {
        use cid::multihash::{Code, MultihashDigest};

        const DIGEST_SIZE: u32 = 32;
        let data = serialize(self, "deal proposal").unwrap();
        let hash = Code::Blake2b256.digest(data.bytes());
        debug_assert_eq!(
            u32::from(hash.size()),
            DIGEST_SIZE,
            "expected 32byte digest"
        );
        Cid::new_v1(DAG_CBOR, hash)
    }
}

impl Deref for Message {
    type Target = MessageV3;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Message {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<MessageV3> for Message {
    fn from(value: MessageV3) -> Self {
        Self(value)
    }
}

impl From<MessageV2> for Message {
    fn from(value: MessageV2) -> Self {
        Self::from(MessageV3 {
            version: value.version as u64,
            from: Address::from(value.from).into(),
            to: Address::from(value.to).into(),
            sequence: value.sequence,
            value: TokenAmount::from(value.value).into(),
            method_num: value.method_num,
            params: value.params.bytes().to_owned().into(),
            gas_limit: value.gas_limit as u64,
            gas_fee_cap: TokenAmount::from(value.gas_fee_cap).into(),
            gas_premium: TokenAmount::from(value.gas_premium).into(),
        })
    }
}

impl From<Message> for MessageV2 {
    fn from(value: Message) -> Self {
        Self {
            version: value.version as i64,
            from: Address::from(value.from).into(),
            to: Address::from(value.to).into(),
            sequence: value.sequence,
            value: TokenAmount::from(value.value.clone()).into(),
            method_num: value.method_num,
            params: value.params.bytes().to_owned().into(),
            gas_limit: value.gas_limit as i64,
            gas_fee_cap: TokenAmount::from(value.gas_fee_cap.clone()).into(),
            gas_premium: TokenAmount::from(value.gas_premium.clone()).into(),
        }
    }
}

impl From<Message> for MessageV3 {
    fn from(value: Message) -> Self {
        value.into()
    }
}
