// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Size of a piece in bytes
#[derive(PartialEq, Debug, Eq, Clone, Copy)]
pub struct UnpaddedPieceSize(pub u64);

impl UnpaddedPieceSize {
    /// Converts unpadded piece size into padded piece size
    pub fn padded(self) -> PaddedPieceSize {
        PaddedPieceSize(self.0 + (self.0 / 127))
    }

    /// Validates piece size
    pub fn validate(self) -> Result<(), &'static str> {
        if self.0 < 127 {
            return Err("minimum piece size is 127 bytes");
        }

        // is 127 * 2^n
        if self.0 >> self.0.trailing_zeros() != 127 {
            return Err("unpadded piece size must be a power of 2 multiple of 127");
        }

        Ok(())
    }
}

/// Size of a piece in bytes with padding
#[derive(PartialEq, Debug, Eq, Clone, Copy, Serialize, Deserialize)]
pub struct PaddedPieceSize(pub u64);

impl PaddedPieceSize {
    /// Converts padded piece size into an unpadded piece size
    pub fn unpadded(self) -> UnpaddedPieceSize {
        UnpaddedPieceSize(self.0 - (self.0 / 128))
    }

    /// Validates piece size
    pub fn validate(self) -> Result<(), &'static str> {
        if self.0 < 128 {
            return Err("minimum piece size is 128 bytes");
        }

        if self.0.count_ones() != 1 {
            return Err("padded piece size must be a power of 2");
        }

        Ok(())
    }
}

// Piece information for part or a whole file
pub struct PieceInfo {
    /// Size in nodes. For BLS12-381 (capacity 254 bits), must be >= 16. (16 * 8 = 128)
    pub size: PaddedPieceSize,
    /// Content identifier for piece
    pub cid: Cid,
}

impl Serialize for PieceInfo {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.size, &self.cid).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PieceInfo {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (size, cid) = Deserialize::deserialize(deserializer)?;
        Ok(Self { size, cid })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_piece_size() {
        let p_piece = PaddedPieceSize(0b10000000);
        p_piece.validate().unwrap();
        let up_piece = p_piece.unpadded();
        up_piece.validate().unwrap();
        assert_eq!(&up_piece, &UnpaddedPieceSize(127));
        assert_eq!(&p_piece, &up_piece.padded());
    }
    #[test]
    fn invalid_piece_checks() {
        let p = PaddedPieceSize(127);
        assert_eq!(p.validate(), Err("minimum piece size is 128 bytes"));
        let p = UnpaddedPieceSize(126);
        assert_eq!(p.validate(), Err("minimum piece size is 127 bytes"));
        let p = PaddedPieceSize(0b10000001);
        assert_eq!(p.validate(), Err("padded piece size must be a power of 2"));
        assert_eq!(UnpaddedPieceSize(0b1111111000).validate(), Ok(()));
        assert_eq!(
            UnpaddedPieceSize(0b1110111000).validate(),
            Err("unpadded piece size must be a power of 2 multiple of 127")
        );
    }
}
