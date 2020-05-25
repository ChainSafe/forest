// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{Message, UnsignedMessage};
use address::Address;
use crypto::{Error as CryptoError, Signature, Signer};
use encoding::tuple::*;
use encoding::Cbor;
use vm::{MethodNum, Serialized, TokenAmount};

/// Represents a wrapped message with signature bytes
#[derive(PartialEq, Clone, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct SignedMessage {
    message: UnsignedMessage,
    signature: Signature,
}

impl SignedMessage {
    pub fn new<S: Signer>(msg: UnsignedMessage, signer: &S) -> Result<Self, CryptoError> {
        let bz = msg.marshal_cbor()?;

        let sig = signer.sign_bytes(bz, msg.from())?;

        Ok(SignedMessage {
            message: msg,
            signature: sig,
        })
    }
    pub fn message(&self) -> &UnsignedMessage {
        &self.message
    }
    pub fn signature(&self) -> &Signature {
        &self.signature
    }
}

impl Message for SignedMessage {
    fn from(&self) -> &Address {
        self.message.from()
    }
    fn to(&self) -> &Address {
        self.message.to()
    }
    fn sequence(&self) -> u64 {
        self.message.sequence()
    }
    fn value(&self) -> &TokenAmount {
        self.message.value()
    }
    fn method_num(&self) -> MethodNum {
        self.message.method_num()
    }
    fn params(&self) -> &Serialized {
        self.message.params()
    }
    fn gas_price(&self) -> &TokenAmount {
        self.message.gas_price()
    }
    fn gas_limit(&self) -> u64 {
        self.message.gas_limit()
    }
    fn required_funds(&self) -> TokenAmount {
        self.message.required_funds()
    }
}

impl Cbor for SignedMessage {}

#[cfg(feature = "json")]
pub mod json {
    use super::*;
    use crate::unsigned_message;
    use crypto::signature;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Wrapper for serializing and deserializing a SignedMessage from JSON.
    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct SignedMessageJson(#[serde(with = "self")] pub SignedMessage);

    /// Wrapper for serializing a SignedMessage reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct SignedMessageJsonRef<'a>(#[serde(with = "self")] pub &'a SignedMessage);

    pub fn serialize<S>(m: &SignedMessage, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct SignedMessageSer<'a> {
            #[serde(rename = "Message", with = "unsigned_message::json")]
            message: &'a UnsignedMessage,
            #[serde(rename = "Signature", with = "signature::json")]
            signature: &'a Signature,
        }
        SignedMessageSer {
            message: &m.message,
            signature: &m.signature,
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SignedMessage, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Serialize, Deserialize)]
        struct SignedMessageDe {
            #[serde(rename = "Message", with = "unsigned_message::json")]
            message: UnsignedMessage,
            #[serde(rename = "Signature", with = "signature::json")]
            signature: Signature,
        }
        let SignedMessageDe { message, signature } = Deserialize::deserialize(deserializer)?;
        Ok(SignedMessage { message, signature })
    }
}
