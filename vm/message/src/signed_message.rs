// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{Message, UnsignedMessage};
use address::Address;
use crypto::{Error as CryptoError, Signature, SignatureType, Signer};
use encoding::tuple::*;
use encoding::{to_vec, Cbor, Error};
use vm::{MethodNum, Serialized, TokenAmount};

/// Represents a wrapped message with signature bytes.
#[derive(PartialEq, Clone, Debug, Serialize_tuple, Deserialize_tuple, Hash, Eq)]
pub struct SignedMessage {
    pub message: UnsignedMessage,
    pub signature: Signature,
}

impl SignedMessage {
    /// Generate new signed message from an unsigned message and a signer.
    pub fn new<S: Signer>(message: UnsignedMessage, signer: &S) -> Result<Self, CryptoError> {
        let bz = message.to_signing_bytes();

        let signature = signer.sign_bytes(&bz, message.from())?;

        Ok(SignedMessage { message, signature })
    }

    /// Generate a new signed message from fields.
    pub fn new_from_parts(
        message: UnsignedMessage,
        signature: Signature,
    ) -> Result<SignedMessage, String> {
        signature.verify(&message.to_signing_bytes(), message.from())?;
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

    /// Consumes self and returns it's unsigned message.
    pub fn into_message(self) -> UnsignedMessage {
        self.message
    }

    /// Checks if the signed message is a BLS message.
    pub fn is_bls(&self) -> bool {
        self.signature.signature_type() == SignatureType::BLS
    }

    /// Checks if the signed message is a Secp256k1 message.
    pub fn is_secp256k1(&self) -> bool {
        self.signature.signature_type() == SignatureType::Secp256k1
    }

    /// Verifies that the from address of the message generated the signature.
    pub fn verify(&self) -> Result<(), String> {
        self.signature
            .verify(&self.message.to_signing_bytes(), self.from())
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
    fn gas_limit(&self) -> i64 {
        self.message.gas_limit()
    }
    fn set_gas_limit(&mut self, token_amount: i64) {
        self.message.set_gas_limit(token_amount)
    }
    fn set_sequence(&mut self, new_sequence: u64) {
        self.message.set_sequence(new_sequence)
    }
    fn required_funds(&self) -> TokenAmount {
        self.message.required_funds()
    }
    fn gas_fee_cap(&self) -> &TokenAmount {
        self.message.gas_fee_cap()
    }
    fn gas_premium(&self) -> &TokenAmount {
        self.message.gas_premium()
    }

    fn set_gas_fee_cap(&mut self, cap: TokenAmount) {
        self.message.set_gas_fee_cap(cap);
    }

    fn set_gas_premium(&mut self, prem: TokenAmount) {
        self.message.set_gas_premium(prem);
    }
}

impl Cbor for SignedMessage {
    fn marshal_cbor(&self) -> Result<Vec<u8>, Error> {
        if self.is_bls() {
            self.message.marshal_cbor()
        } else {
            Ok(to_vec(&self)?)
        }
    }
}

#[cfg(feature = "json")]
pub mod json {
    use super::*;
    use crate::unsigned_message;
    use cid::Cid;
    use crypto::signature;
    use serde::{ser, Deserialize, Deserializer, Serialize, Serializer};

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

    impl From<SignedMessage> for SignedMessageJson {
        fn from(msg: SignedMessage) -> Self {
            SignedMessageJson(msg)
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
            #[serde(default, rename = "CID", with = "cid::json::opt")]
            cid: Option<Cid>,
        }
        SignedMessageSer {
            message: &m.message,
            signature: &m.signature,
            cid: Some(m.cid().map_err(ser::Error::custom)?),
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
