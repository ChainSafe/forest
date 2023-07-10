// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::{
    address::Address,
    crypto::{Signature, SignatureType},
    econ::TokenAmount,
    message::Message,
};
use fvm_ipld_encoding::RawBytes;
use fvm_shared::MethodNum;
use serde_tuple::{self, Deserialize_tuple, Serialize_tuple};

use super::Message as MessageTrait;

/// Represents a wrapped message with signature bytes.
#[derive(PartialEq, Clone, Debug, Serialize_tuple, Deserialize_tuple, Hash, Eq)]
pub struct SignedMessage {
    pub message: Message,
    pub signature: Signature,
}

impl SignedMessage {
    /// Generate a new signed message from fields.
    /// The signature will be verified.
    pub fn new_from_parts(message: Message, signature: Signature) -> anyhow::Result<SignedMessage> {
        signature
            .verify(&message.cid()?.to_bytes(), &message.from())
            .map_err(anyhow::Error::msg)?;
        Ok(SignedMessage { message, signature })
    }

    /// Generate a new signed message from fields.
    /// The signature will not be verified.
    pub fn new_unchecked(message: Message, signature: Signature) -> SignedMessage {
        SignedMessage { message, signature }
    }

    /// Returns reference to the unsigned message.
    pub fn message(&self) -> &Message {
        &self.message
    }

    /// Returns signature of the signed message.
    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    /// Consumes self and returns it's unsigned message.
    pub fn into_message(self) -> Message {
        self.message
    }

    /// Checks if the signed message is a BLS message.
    pub fn is_bls(&self) -> bool {
        self.signature.signature_type() == SignatureType::Bls
    }

    /// Checks if the signed message is a SECP message.
    pub fn is_secp256k1(&self) -> bool {
        self.signature.signature_type() == SignatureType::Secp256k1
    }

    /// Checks if the signed message is a delegated message.
    pub fn is_delegated(&self) -> bool {
        self.signature.signature_type() == SignatureType::Delegated
    }

    /// Verifies that the from address of the message generated the signature.
    pub fn verify(&self) -> Result<(), String> {
        self.signature
            .verify(&self.message.cid().unwrap().to_bytes(), &self.from())
    }

    // Important note: `msg.cid()` is different from
    // `Cid::from_cbor_blake2b256(msg)`. The behavior comes from Lotus, and
    // Lotus, by, definition, is correct.
    pub fn cid(&self) -> Result<cid::Cid, fvm_ipld_encoding::Error> {
        if self.is_bls() {
            self.message.cid()
        } else {
            use crate::utils::cid::CidCborExt;
            cid::Cid::from_cbor_blake2b256(self)
        }
    }
}

#[cfg(test)]
impl quickcheck::Arbitrary for SignedMessage {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        SignedMessage {
            message: Message::arbitrary(g),
            signature: Signature::arbitrary(g),
        }
    }
}

impl MessageTrait for SignedMessage {
    fn from(&self) -> Address {
        self.message.from()
    }
    fn to(&self) -> Address {
        self.message.to()
    }
    fn sequence(&self) -> u64 {
        self.message.sequence()
    }
    fn value(&self) -> TokenAmount {
        self.message.value()
    }
    fn method_num(&self) -> MethodNum {
        self.message.method_num
    }
    fn params(&self) -> &RawBytes {
        self.message.params()
    }
    fn gas_limit(&self) -> u64 {
        self.message.gas_limit()
    }
    fn set_gas_limit(&mut self, token_amount: u64) {
        self.message.set_gas_limit(token_amount);
    }
    fn set_sequence(&mut self, new_sequence: u64) {
        self.message.set_sequence(new_sequence);
    }
    fn required_funds(&self) -> TokenAmount {
        self.message.required_funds()
    }
    fn gas_fee_cap(&self) -> TokenAmount {
        self.message.gas_fee_cap()
    }
    fn gas_premium(&self) -> TokenAmount {
        self.message.gas_premium()
    }

    fn set_gas_fee_cap(&mut self, cap: TokenAmount) {
        self.message.set_gas_fee_cap(cap)
    }

    fn set_gas_premium(&mut self, prem: TokenAmount) {
        self.message.set_gas_premium(prem)
    }
}
