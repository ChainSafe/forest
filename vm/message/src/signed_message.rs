// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_encoding::tuple::*;
use forest_shim::{address::Address, econ::TokenAmount, message::Message};
use fvm_ipld_encoding::{to_vec, Cbor, Error as CborError, RawBytes};
use fvm_shared::{
    crypto::signature::{Signature, SignatureType},
    MethodNum,
};

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
            .verify(&message.cid()?.to_bytes(), &message.from)
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
        self.signature.signature_type() == SignatureType::BLS
    }

    /// Checks if the signed message is a SECP message.
    pub fn is_secp256k1(&self) -> bool {
        self.signature.signature_type() == SignatureType::Secp256k1
    }

    /// Verifies that the from address of the message generated the signature.
    pub fn verify(&self) -> Result<(), String> {
        self.signature
            .verify(&self.message.cid().unwrap().to_bytes(), &self.from().into())
    }
}

impl MessageTrait for SignedMessage {
    fn from(&self) -> Address {
        Address::from(self.message.from)
    }
    fn to(&self) -> Address {
        Address::from(self.message.to)
    }
    fn sequence(&self) -> u64 {
        self.message.sequence
    }
    fn value(&self) -> TokenAmount {
        TokenAmount::from(&self.message.value)
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
        TokenAmount::from(&self.message.gas_fee_cap * self.message.gas_limit + &self.message.value)
    }
    fn gas_fee_cap(&self) -> TokenAmount {
        TokenAmount::from(&self.message.gas_fee_cap)
    }
    fn gas_premium(&self) -> TokenAmount {
        TokenAmount::from(&self.message.gas_premium)
    }

    fn set_gas_fee_cap(&mut self, cap: TokenAmount) {
        self.message.gas_fee_cap = cap.into();
    }

    fn set_gas_premium(&mut self, prem: TokenAmount) {
        self.message.gas_premium = prem.into();
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
