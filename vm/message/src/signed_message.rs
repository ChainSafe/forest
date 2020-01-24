// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use super::{Message, UnsignedMessage};
use address::Address;
use cid::{Cid, Codec, Error as CidError, Version};
use crypto::{Error as CryptoError, Signature, Signer};
use encoding::{Cbor, Error as EncodingError};
use multihash::Multihash;
use num_bigint::BigUint;
use raw_block::RawBlock;
use serde::{Deserialize, Serialize};
use vm::{MethodNum, Serialized, TokenAmount};

/// Represents a wrapped message with signature bytes
#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct SignedMessage {
    message: UnsignedMessage,
    signature: Signature,
}

// TODO verify format or implement custom serialize/deserialize function (if necessary):
// https://github.com/ChainSafe/ferret/issues/143

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
    fn method_num(&self) -> &MethodNum {
        self.message.method_num()
    }
    fn params(&self) -> &Serialized {
        self.message.params()
    }
    fn gas_price(&self) -> &BigUint {
        self.message.gas_price()
    }
    fn gas_limit(&self) -> &BigUint {
        self.message.gas_limit()
    }
}

impl RawBlock for SignedMessage {
    fn raw_data(&self) -> Result<Vec<u8>, EncodingError> {
        // TODO should serialize message using CBOR encoding
        self.marshal_cbor()
    }
    fn cid(&self) -> Result<Cid, CidError> {
        let hash = Multihash::from_bytes(self.marshal_cbor()?)?;
        Ok(Cid::new(Codec::DagCBOR, Version::V1, hash))
    }
}

impl Cbor for SignedMessage {}
