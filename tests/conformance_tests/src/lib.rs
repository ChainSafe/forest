// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![cfg(feature = "submodule_tests")]

mod message;
mod stubs;
mod tipset;

pub use self::message::*;
pub use self::stubs::*;
pub use self::tipset::*;
use actor::CHAOS_ACTOR_CODE_ID;
use address::{Address, Protocol};
use blockstore::BlockStore;
use cid::Cid;
use clock::ChainEpoch;
use crypto::{DomainSeparationTag, Signature};
use encoding::Cbor;
use fil_types::{SealVerifyInfo, WindowPoStVerifyInfo};
use forest_message::{ChainMessage, Message, MessageReceipt, SignedMessage, UnsignedMessage};
use interpreter::{ApplyRet, BlockMessages, Rand, VM};
use num_bigint::BigInt;
use runtime::{ConsensusFault, Syscalls};
use serde::{Deserialize, Deserializer};
use std::error::Error as StdError;
use vm::{ExitCode, Serialized};

const BASE_FEE: u64 = 100;

mod base64_bytes {
    use super::*;
    use serde::de;
    use std::borrow::Cow;

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: Cow<'de, str> = Deserialize::deserialize(deserializer)?;
        Ok(base64::decode(s.as_ref()).map_err(de::Error::custom)?)
    }

    pub mod vec {
        use super::*;

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Vec<u8>>, D::Error>
        where
            D: Deserializer<'de>,
        {
            let v: Vec<Cow<'de, str>> = Deserialize::deserialize(deserializer)?;
            Ok(v.into_iter()
                .map(|s| base64::decode(s.as_ref()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(de::Error::custom)?)
        }
    }
}

mod message_receipt_vec {
    use super::*;

    #[derive(Deserialize)]
    pub struct MessageReceiptVector {
        exit_code: ExitCode,
        #[serde(rename = "return", with = "base64_bytes")]
        return_value: Vec<u8>,
        gas_used: i64,
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<MessageReceipt>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: Vec<MessageReceiptVector> = Deserialize::deserialize(deserializer)?;
        Ok(s.into_iter()
            .map(|v| MessageReceipt {
                exit_code: v.exit_code,
                return_data: Serialized::new(v.return_value),
                gas_used: v.gas_used,
            })
            .collect())
    }
}

#[derive(Debug, Deserialize)]
pub struct StateTreeVector {
    #[serde(with = "cid::json")]
    pub root_cid: Cid,
}

#[derive(Debug, Deserialize)]
pub struct GenerationData {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Deserialize)]
pub struct MetaData {
    pub id: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub comment: String,
    pub gen: Vec<GenerationData>,
}

#[derive(Debug, Deserialize)]
pub struct PreConditions {
    pub epoch: ChainEpoch,
    pub state_tree: StateTreeVector,
}

#[derive(Debug, Deserialize)]
pub struct PostConditions {
    pub state_tree: StateTreeVector,
    #[serde(with = "message_receipt_vec")]
    pub receipts: Vec<MessageReceipt>,
    #[serde(default, with = "cid::json::vec")]
    pub receipts_roots: Vec<Cid>,
}

#[derive(Debug, Deserialize)]
pub struct Selector {
    #[serde(default)]
    pub puppet_actor: Option<String>,
    #[serde(default)]
    pub chaos_actor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "class")]
pub enum TestVector {
    #[serde(rename = "message")]
    Message {
        selector: Option<Selector>,
        #[serde(rename = "_meta")]
        meta: Option<MetaData>,

        #[serde(with = "base64_bytes")]
        car: Vec<u8>,
        preconditions: PreConditions,
        apply_messages: Vec<MessageVector>,
        postconditions: PostConditions,
    },
    #[serde(rename = "block")]
    Block {
        selector: Option<Selector>,
        #[serde(rename = "_meta")]
        meta: Option<MetaData>,
    },
    #[serde(rename = "tipset")]
    Tipset {
        selector: Option<Selector>,
        #[serde(rename = "_meta")]
        meta: Option<MetaData>,

        #[serde(with = "base64_bytes")]
        car: Vec<u8>,
        preconditions: PreConditions,
        apply_tipsets: Vec<TipsetVector>,
        postconditions: PostConditions,
    },
    #[serde(rename = "chain")]
    Chain {
        selector: Option<Selector>,
        #[serde(rename = "_meta")]
        meta: Option<MetaData>,
    },
}

// This might be changed to be encoded into vector, matching go runner for now
pub fn to_chain_msg(msg: UnsignedMessage) -> ChainMessage {
    if msg.from().protocol() == Protocol::Secp256k1 {
        ChainMessage::Signed(SignedMessage {
            message: msg,
            signature: Signature::new_secp256k1(vec![0; 65]),
        })
    } else {
        ChainMessage::Unsigned(msg)
    }
}
