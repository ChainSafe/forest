// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{BitField, Result};
use encoding::serde_bytes;
use serde::{Deserialize, Deserializer, Serialize};

/// A trait for types that can produce a `&BitField` (or fail to do so).
/// Generalizes over `&BitField` and `&mut UnvalidatedBitField`.
pub trait Validate<'a> {
    fn validate(self) -> Result<&'a BitField>;
    fn validate_with_max(self) -> Result<(&'a BitField, u64)>;
}

impl<'a> Validate<'a> for &'a mut UnvalidatedBitField {
    /// Validates the RLE+ encoding of the bit field, returning a shared
    /// reference to the decoded bit field.
    fn validate(self) -> Result<&'a BitField> {
        self.validate_mut().map(|bf| &*bf)
    }
    /// it's O(1) to get max set value in bitfield during validation, so we do that here.
    fn validate_with_max(self) -> Result<(&'a BitField, u64)> {
        self.validate_mut_with_max().map(|(bf,max)| (&*bf,max))
    }
}

impl<'a> Validate<'a> for &'a BitField {
    fn validate(self) -> Result<&'a BitField> {
        Ok(self)
    }
    /// unimplemented because it's slow- this function exists to exploit that unvalidated
    /// bitfields can get their max value quickly during validation
    fn validate_with_max(self) -> Result<(&'a BitField, u64)> { unimplemented!(); }
}

/// A bit field that may not yet have been validated for valid RLE+.
/// Used to defer this validation step until when the bit field is
/// first used, rather than at deserialization.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum UnvalidatedBitField {
    Validated(BitField),
    Unvalidated(#[serde(with = "serde_bytes")] Vec<u8>),
}

impl UnvalidatedBitField {
    /// validates the RLE+ encoding of the bitfield, returning a unique reference to the
    /// decoded bit field. calls validate_mut_with_max and throws away the max returned
    pub fn validate_mut(&mut self) -> Result<&mut BitField> {
        self.validate_mut_with_max().map(|(bf, _)| bf)
    }

    /// Validates the RLE+ encoding of the bit field, returning a unique
    /// reference to the decoded bit field, and saving the maximum thing in the RLE input bitfield.
    /// this can be useful for doing quick validation stuff in code that uses this library
    pub fn validate_mut_with_max(&mut self) -> Result<(&mut BitField, u64)> {
        let max = if let Self::Unvalidated(bytes) = self {
            let (bf, max) = BitField::from_bytes_with_max(bytes)?;
            *self = Self::Validated(bf);
            max
        } else { unreachable!() };

        match self {
            Self::Validated(bf) => Ok((bf, max)),
            Self::Unvalidated(_) => unreachable!(),
        }
    }
}

impl From<BitField> for UnvalidatedBitField {
    fn from(bf: BitField) -> Self {
        Self::Validated(bf)
    }
}

impl<'de> Deserialize<'de> for UnvalidatedBitField {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes: Vec<u8> = serde_bytes::deserialize(deserializer)?;
        Ok(Self::Unvalidated(bytes))
    }
}
