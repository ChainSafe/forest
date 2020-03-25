// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::SectorSize;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

/// This ordering, defines mappings to UInt in a way which MUST never change.
#[derive(PartialEq, Eq, Copy, Clone, FromPrimitive, Debug, Hash)]
#[repr(u8)]
pub enum RegisteredProof {
    StackedDRG32GiBSeal = 1,
    StackedDRG32GiBPoSt = 2,
    StackedDRG2KiBSeal = 3,
    StackedDRG2KiBPoSt = 4,
    StackedDRG8MiBSeal = 5,
    StackedDRG8MiBPoSt = 6,
    StackedDRG512MiBSeal = 7,
    StackedDRG512MiBPoSt = 8,
}

impl RegisteredProof {
    pub fn from_byte(b: u8) -> Option<Self> {
        FromPrimitive::from_u8(b)
    }

    /// Returns the sector size of the proof type, which is measured in bytes.
    pub fn sector_size(self) -> SectorSize {
        use RegisteredProof::*;
        match self {
            StackedDRG32GiBSeal | StackedDRG32GiBPoSt => SectorSize::_32GiB,
            StackedDRG2KiBSeal | StackedDRG2KiBPoSt => SectorSize::_2KiB,
            StackedDRG8MiBSeal | StackedDRG8MiBPoSt => SectorSize::_8MiB,
            StackedDRG512MiBSeal | StackedDRG512MiBPoSt => SectorSize::_512MiB,
        }
    }

    /// RegisteredPoStProof produces the PoSt-specific RegisteredProof corresponding
    /// to the receiving RegisteredProof.
    pub fn registered_post_proof(self) -> RegisteredProof {
        use RegisteredProof::*;
        match self {
            StackedDRG32GiBSeal => StackedDRG32GiBPoSt,
            StackedDRG2KiBSeal => StackedDRG2KiBPoSt,
            StackedDRG8MiBSeal => StackedDRG8MiBPoSt,
            StackedDRG512MiBSeal => StackedDRG512MiBPoSt,
            p => p,
        }
    }

    /// RegisteredSealProof produces the seal-specific RegisteredProof corresponding
    /// to the receiving RegisteredProof.
    pub fn registered_seal_proof(self) -> RegisteredProof {
        use RegisteredProof::*;
        match self {
            StackedDRG32GiBPoSt => StackedDRG32GiBSeal,
            StackedDRG2KiBPoSt => StackedDRG2KiBSeal,
            StackedDRG8MiBPoSt => StackedDRG8MiBSeal,
            StackedDRG512MiBPoSt => StackedDRG512MiBSeal,
            p => p,
        }
    }
}

impl Default for RegisteredProof {
    fn default() -> Self {
        RegisteredProof::StackedDRG2KiBPoSt
    }
}

impl Serialize for RegisteredProof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (*self as u8).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RegisteredProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let b: u8 = Deserialize::deserialize(deserializer)?;
        Ok(Self::from_byte(b).ok_or_else(|| de::Error::custom("Invalid registered proof byte"))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use encoding::*;

    #[test]
    fn round_trip_proof_ser() {
        let bz = to_vec(&RegisteredProof::StackedDRG512MiBSeal).unwrap();
        let proof: RegisteredProof = from_slice(&bz).unwrap();
        assert_eq!(proof, RegisteredProof::StackedDRG512MiBSeal);
    }
}
