// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{Message, UnsignedMessage};
use address::Address;
use crypto::{Error as CryptoError, Signature, Signer};
use encoding::Cbor;
use encoding::{de::Deserializer, ser::Serializer};
use serde::{Deserialize, Serialize};
use vm::{MethodNum, Serialized, TokenAmount};

/// Represents a wrapped message with signature bytes
#[derive(PartialEq, Clone, Debug)]
pub struct SignedMessage {
    message: UnsignedMessage,
    signature: Signature,
}

impl Serialize for SignedMessage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.message, &self.signature).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SignedMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (message, signature) = Deserialize::deserialize(deserializer)?;
        Ok(SignedMessage { message, signature })
    }
}

impl SignedMessage {
    pub fn new<S: Signer>(msg: &UnsignedMessage, signer: &S) -> Result<Self, CryptoError> {
        let bz = msg.marshal_cbor()?;

        let sig = signer.sign_bytes(bz, msg.from())?;

        Ok(SignedMessage {
            message: msg.clone(),
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
