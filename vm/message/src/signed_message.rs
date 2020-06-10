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
    /// Generate new signed message from an unsigned message and a signer.
    pub fn new<S: Signer>(message: UnsignedMessage, signer: &S) -> Result<Self, CryptoError> {
        let bz = message.marshal_cbor()?;

        let signature = signer.sign_bytes(bz, message.from())?;

        Ok(SignedMessage { message, signature })
    }

    /// Returns reference to the unsigned message.
    pub fn message(&self) -> &UnsignedMessage {
        &self.message
    }

    /// Returns signature of the signed message.
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

    impl From<SignedMessageJson> for SignedMessage {
        fn from(wrapper: SignedMessageJson) -> Self {
            wrapper.0
        }
    }

    pub fn serialize<S>(m: &SignedMessage, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        #[serde(rename_all = "PascalCase")]
        struct SignedMessageSer<'a> {
            #[serde(with = "unsigned_message::json")]
            message: &'a UnsignedMessage,
            #[serde(with = "signature::json")]
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
        #[serde(rename_all = "PascalCase")]
        struct SignedMessageDe {
            #[serde(with = "unsigned_message::json")]
            message: UnsignedMessage,
            #[serde(with = "signature::json")]
            signature: Signature,
        }
        let SignedMessageDe { message, signature } = Deserialize::deserialize(deserializer)?;
        Ok(SignedMessage { message, signature })
    }

    pub mod vec {
        use super::*;
        use forest_json_utils::GoVecVisitor;
        use serde::ser::SerializeSeq;

        pub fn serialize<S>(m: &[SignedMessage], serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut seq = serializer.serialize_seq(Some(m.len()))?;
            for e in m {
                seq.serialize_element(&SignedMessageJsonRef(e))?;
            }
            seq.end()
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<SignedMessage>, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(GoVecVisitor::<SignedMessage, SignedMessageJson>::new())
        }
    }
}
