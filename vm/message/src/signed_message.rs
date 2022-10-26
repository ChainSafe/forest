// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Message as MessageTrait;
use forest_crypto::Signer;
use forest_encoding::tuple::*;
use fvm_ipld_encoding::{to_vec, Cbor, Error as CborError, RawBytes};
use fvm_shared::address::Address;
use fvm_shared::crypto::signature::{Error as CryptoError, Signature, SignatureType};
use fvm_shared::econ::TokenAmount;
use fvm_shared::message::Message;
use fvm_shared::MethodNum;

/// Represents a wrapped message with signature bytes.
#[derive(PartialEq, Clone, Debug, Serialize_tuple, Deserialize_tuple, Hash, Eq)]
pub struct SignedMessage {
    pub message: Message,
    pub signature: Signature,
}

impl SignedMessage {
    /// Generate new signed message from an unsigned message and a signer.
    pub fn new<S: Signer>(message: Message, signer: &S) -> Result<Self, CryptoError> {
        let bz = message.cid()?.to_bytes();

        let signature = signer
            .sign_bytes(&bz, &message.from)
            .map_err(|e| CryptoError::SigningError(e.to_string()))?;

        Ok(SignedMessage { message, signature })
    }

    /// Generate a new signed message from fields.
    pub fn new_from_parts(message: Message, signature: Signature) -> Result<SignedMessage, String> {
        signature.verify(&message.cid().unwrap().to_bytes(), &message.from)?;
        Ok(SignedMessage { message, signature })
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
        self.signature.signature_type() == SignatureType::BLS
    }

    /// Checks if the signed message is a SECP message.
    pub fn is_secp256k1(&self) -> bool {
        self.signature.signature_type() == SignatureType::Secp256k1
    }

    /// Verifies that the from address of the message generated the signature.
    pub fn verify(&self) -> Result<(), String> {
        self.signature
            .verify(&self.message.cid().unwrap().to_bytes(), self.from())
    }
}

impl MessageTrait for SignedMessage {
    fn from(&self) -> &Address {
        &self.message.from
    }
    fn to(&self) -> &Address {
        &self.message.to
    }
    fn sequence(&self) -> u64 {
        self.message.sequence
    }
    fn value(&self) -> &TokenAmount {
        &self.message.value
    }
    fn method_num(&self) -> MethodNum {
        self.message.method_num
    }
    fn params(&self) -> &RawBytes {
        &self.message.params
    }
    fn gas_limit(&self) -> i64 {
        self.message.gas_limit
    }
    fn set_gas_limit(&mut self, token_amount: i64) {
        self.message.gas_limit = token_amount;
    }
    fn set_sequence(&mut self, new_sequence: u64) {
        self.message.sequence = new_sequence;
    }
    fn required_funds(&self) -> TokenAmount {
        &self.message.gas_fee_cap * self.message.gas_limit + &self.message.value
    }
    fn gas_fee_cap(&self) -> &TokenAmount {
        &self.message.gas_fee_cap
    }
    fn gas_premium(&self) -> &TokenAmount {
        &self.message.gas_premium
    }

    fn set_gas_fee_cap(&mut self, cap: TokenAmount) {
        self.message.gas_fee_cap = cap;
    }

    fn set_gas_premium(&mut self, prem: TokenAmount) {
        self.message.gas_premium = prem;
    }
}

impl Cbor for SignedMessage {
    fn marshal_cbor(&self) -> Result<Vec<u8>, CborError> {
        if self.is_bls() {
            self.message.marshal_cbor()
        } else {
            Ok(to_vec(&self)?)
        }
    }
}
