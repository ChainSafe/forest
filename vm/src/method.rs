// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use encoding::{ser, to_vec, Error as EncodingError};
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};

/// Method number indicator for calling actor methods
#[derive(Default, Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct MethodNum(i32); // TODO: add constraints to this

// TODO verify format or implement custom serialize/deserialize function (if necessary):
// https://github.com/ChainSafe/ferret/issues/143

impl MethodNum {
    /// Constructor for new MethodNum
    pub fn new(num: i32) -> Self {
        Self(num)
    }
}

impl From<MethodNum> for i32 {
    fn from(method_num: MethodNum) -> i32 {
        method_num.0
    }
}

/// Base actor send method
pub const METHOD_SEND: isize = 0;
/// Base actor constructor method
pub const METHOD_CONSTRUCTOR: isize = 1;
/// Base actor cron method
pub const METHOD_CRON: isize = 2;

/// Placeholder for non base methods for actors
// TODO revisit on complete spec
pub const METHOD_PLACEHOLDER: isize = 3;

/// Serialized bytes to be used as individual parameters into actor methods
#[derive(Default, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Serialized {
    bytes: Vec<u8>,
}

// TODO verify format or implement custom serialize/deserialize function (if necessary):
// https://github.com/ChainSafe/ferret/issues/143

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

    /// Contructor for encoding Cbor encodable structure
    pub fn serialize<O: ser::Serialize>(obj: O) -> Result<Self, EncodingError> {
        Ok(Self {
            bytes: to_vec(&obj)?,
        })
    }

    /// Returns serialized bytes
    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }
}

/// Method parameters used in Actor execution
#[derive(Default, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct MethodParams {
    params: Vec<Serialized>,
}

// TODO verify format or implement custom serialize/deserialize function (if necessary):
// https://github.com/ChainSafe/ferret/issues/143

impl Deref for MethodParams {
    type Target = Vec<Serialized>;
    fn deref(&self) -> &Self::Target {
        &self.params
    }
}

impl DerefMut for MethodParams {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.params
    }
}
