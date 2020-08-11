// Copyright 2020 ChainSafe Systems
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
pub enum ChainMessage {
    Unsigned(UnsignedMessage),
    Signed(SignedMessage),
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
    fn gas_price(&self) -> &TokenAmount {
        match self {
            Self::Signed(t) => t.gas_price(),
            Self::Unsigned(t) => t.gas_price(),
        }
    }
    fn set_gas_price(&mut self, token_amount: TokenAmount) {
        match self {
            Self::Signed(t) => t.set_gas_price(token_amount),
            Self::Unsigned(t) => t.set_gas_price(token_amount),
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

#[cfg(feature = "json")]
pub mod json {
    use super::*;
    use crate::{signed_message, unsigned_message};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Wrapper for serializing and deserializing a SignedMessage from JSON.
    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct ChainMessageJson(#[serde(with = "self")] pub ChainMessage);

    /// Wrapper for serializing a SignedMessage reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct ChainMessageJsonRef<'a>(#[serde(with = "self")] pub &'a ChainMessage);

    impl From<ChainMessageJson> for ChainMessage {
        fn from(wrapper: ChainMessageJson) -> Self {
            wrapper.0
        }
    }

    impl From<ChainMessage> for ChainMessageJson {
        fn from(msg: ChainMessage) -> Self {
            ChainMessageJson(msg)
        }
    }

    pub fn serialize<S>(m: &ChainMessage, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        enum ChainMessageSer {
            #[serde(with = "unsigned_message::json")]
            Unsigned(UnsignedMessage),
            #[serde(with = "signed_message::json")]
            Signed(SignedMessage),
        };

        let chain_message_ser = match m {
            ChainMessage::Unsigned(s) => ChainMessageSer::Unsigned(s.clone()),
            ChainMessage::Signed(s) => ChainMessageSer::Signed(s.clone()),
        };
        chain_message_ser.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ChainMessage, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Serialize, Deserialize)]
        enum ChainMessageDe {
            #[serde(with = "unsigned_message::json")]
            Unsigned(UnsignedMessage),
            #[serde(with = "signed_message::json")]
            Signed(SignedMessage),
        };
        let chain_message: ChainMessageDe = Deserialize::deserialize(deserializer)?;
        Ok(match chain_message {
            ChainMessageDe::Unsigned(s) => ChainMessage::Unsigned(s.to_owned()),
            ChainMessageDe::Signed(s) => ChainMessage::Signed(s.to_owned()),
        })
    }
}
