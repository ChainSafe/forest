// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use super::{Message, UnsignedMessage};
use address::Address;
use crypto::{Error as CryptoError, Signature, Signer};
use encoding::Cbor;
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use vm::{MethodNum, MethodParams, TokenAmount};

/// SignedMessage represents a wrapped message with signature bytes
#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct SignedMessage {
    message: UnsignedMessage,
    signature: Signature,
}

// TODO verify format or implement custom serialize/deserialize function (if necessary):
// https://github.com/ChainSafe/ferret/issues/143

impl SignedMessage {
    pub fn new(msg: &UnsignedMessage, s: &impl Signer) -> Result<SignedMessage, CryptoError> {
        let bz = msg.marshal_cbor()?;

        let sig = s.sign_bytes(bz, msg.from())?;

        Ok(SignedMessage {
            message: msg.clone(),
            signature: sig,
        })
    }
    pub fn message(&self) -> UnsignedMessage {
        self.message.clone()
    }
    pub fn signature(&self) -> Signature {
        self.signature.clone()
    }
}

impl Message for SignedMessage {
    /// from returns the from address of the message
    fn from(&self) -> Address {
        self.message.from()
    }
    /// to returns the destination address of the message
    fn to(&self) -> Address {
        self.message.to()
    }
    /// sequence returns the message sequence or nonce
    fn sequence(&self) -> u64 {
        self.message.sequence()
    }
    /// value returns the amount sent in message
    fn value(&self) -> TokenAmount {
        self.message.value()
    }
    /// method_num returns the method number to be called
    fn method_num(&self) -> MethodNum {
        self.message.method_num()
    }
    /// params returns the encoded parameters for the method call
    fn params(&self) -> MethodParams {
        self.message.params()
    }
    /// gas_price returns gas price for the message
    fn gas_price(&self) -> BigUint {
        self.message.gas_price()
    }
    /// gas_limit returns the gas limit for the message
    fn gas_limit(&self) -> BigUint {
        self.message.gas_limit()
    }
}

// TODO modify signed message encoding format when needed
impl Cbor for SignedMessage {}
