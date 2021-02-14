// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use encoding::{de, from_slice, ser, serde_bytes, to_vec, Cbor, Error as EncodingError};
use serde::{Deserialize, Serialize};
use std::ops::Deref;

/// Method number indicator for calling actor methods.
pub type MethodNum = u64;

/// Base actor send method.
pub const METHOD_SEND: MethodNum = 0;
/// Base actor constructor method.
pub const METHOD_CONSTRUCTOR: MethodNum = 1;

/// Serialized bytes to be used as parameters into actor methods.
/// This data is (de)serialized as a byte string.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, Hash, Eq, Default)]
#[serde(transparent)]
pub struct Serialized {
    #[serde(with = "serde_bytes")]
    bytes: Vec<u8>,
}

impl Cbor for Serialized {}

impl Deref for Serialized {
    type Target = Vec<u8>;
    fn deref(&self) -> &Self::Target {
        &self.bytes
    }
}

impl Serialized {
    /// Constructor if data is encoded already
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    /// Contructor for encoding Cbor encodable structure.
    pub fn serialize<O: ser::Serialize>(obj: O) -> Result<Self, EncodingError> {
        Ok(Self {
            bytes: to_vec(&obj)?,
        })
    }

    /// Returns serialized bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Deserializes the serialized bytes into a defined type.
    pub fn deserialize<O: de::DeserializeOwned>(&self) -> Result<O, EncodingError> {
        Ok(from_slice(&self.bytes)?)
    }
}
