use super::{BitField, BitVec, RLEPlus, Result};
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

/// An RLE+ encoded bit field that may or may not be encoded correctly.
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub struct UnverifiedBitField(#[serde(with = "serde_bytes")] Vec<u8>);

impl UnverifiedBitField {
    /// Returns a verified `BitField` if the data has a valid RLE+ encoding,
    /// and an error otherwise.
    pub fn verify(self) -> Result<BitField> {
        let bitvec = RLEPlus::new(BitVec::from(self.0))?;
        Ok(bitvec.into())
    }
}

impl From<BitField> for UnverifiedBitField {
    fn from(bit_field: BitField) -> Self {
        Self(RLEPlus::from(bit_field).into_bytes())
    }
}

impl TryFrom<UnverifiedBitField> for BitField {
    type Error = &'static str;

    fn try_from(bit_field: UnverifiedBitField) -> Result<Self> {
        bit_field.verify()
    }
}
