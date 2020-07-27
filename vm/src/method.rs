// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::EMPTY_ARR_BYTES;
use encoding::{de, from_slice, ser, serde_bytes, to_vec, Cbor, Error as EncodingError};
use serde::{Deserialize, Serialize};
use std::ops::Deref;

/// Method number indicator for calling actor methods
pub type MethodNum = u64;

/// Base actor send method
pub const METHOD_SEND: MethodNum = 0;
/// Base actor constructor method
pub const METHOD_CONSTRUCTOR: MethodNum = 1;

/// Serialized bytes to be used as parameters into actor methods
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize, Hash, Eq)]
#[serde(transparent)]
pub struct Serialized {
    #[serde(with = "serde_bytes")]
    bytes: Vec<u8>,
}

impl Default for Serialized {
    /// Default serialized bytes is an empty array serialized
    #[inline]
    fn default() -> Self {
        Self {
            bytes: EMPTY_ARR_BYTES.clone(),
        }
    }
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

    /// Empty bytes constructor. Used for empty return values.
    pub fn empty() -> Self {
        Self { bytes: Vec::new() }
    }

    /// Contructor for encoding Cbor encodable structure
    pub fn serialize<O: ser::Serialize>(obj: O) -> Result<Self, EncodingError> {
        Ok(Self {
            bytes: to_vec(&obj)?,
        })
    }

    /// Returns serialized bytes
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Deserializes into a defined type
    pub fn deserialize<O: de::DeserializeOwned>(&self) -> Result<O, EncodingError> {
        Ok(from_slice(&self.bytes)?)
    }
}
