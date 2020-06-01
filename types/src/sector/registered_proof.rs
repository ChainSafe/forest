// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::SectorSize;
use encoding::repr::*;
use filecoin_proofs_api::{RegisteredPoStProof, RegisteredSealProof};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

/// This ordering, defines mappings to UInt in a way which MUST never change.
#[derive(
    PartialEq, Eq, Copy, Clone, FromPrimitive, Debug, Hash, Serialize_repr, Deserialize_repr,
)]
#[repr(u8)]
pub enum RegisteredProof {
    StackedDRG32GiBSeal = 1,
    StackedDRG32GiBPoSt = 2, // TODO unused (revisit if being removed)
    StackedDRG2KiBSeal = 3,
    StackedDRG2KiBPoSt = 4, // TODO unused (revisit if being removed)
    StackedDRG8MiBSeal = 5,
    StackedDRG8MiBPoSt = 6, // TODO unused (revisit if being removed)
    StackedDRG512MiBSeal = 7,
    StackedDRG512MiBPoSt = 8, // TODO unused (revisit if being removed)

    StackedDRG2KiBWinningPoSt = 9,
    StackedDRG2KiBWindowPoSt = 10,

    StackedDRG8MiBWinningPoSt = 11,
    StackedDRG8MiBWindowPoSt = 12,

    StackedDRG512MiBWinningPoSt = 13,
    StackedDRG512MiBWindowPoSt = 14,

    StackedDRG32GiBWinningPoSt = 15,
    StackedDRG32GiBWindowPoSt = 16,

    StackedDRG64GiBSeal = 17,

    StackedDRG64GiBWinningPoSt = 18,
    StackedDRG64GiBWindowPoSt = 19,
}

impl RegisteredProof {
    pub fn from_byte(b: u8) -> Option<Self> {
        FromPrimitive::from_u8(b)
    }

    /// Returns the sector size of the proof type, which is measured in bytes.
    pub fn sector_size(self) -> SectorSize {
        use RegisteredProof::*;
        match self {
            StackedDRG64GiBSeal | StackedDRG64GiBWindowPoSt | StackedDRG64GiBWinningPoSt => {
                SectorSize::_64GiB
            }
            StackedDRG32GiBSeal
            | StackedDRG32GiBPoSt
            | StackedDRG32GiBWindowPoSt
            | StackedDRG32GiBWinningPoSt => SectorSize::_32GiB,
            StackedDRG2KiBSeal
            | StackedDRG2KiBPoSt
            | StackedDRG2KiBWindowPoSt
            | StackedDRG2KiBWinningPoSt => SectorSize::_2KiB,
            StackedDRG8MiBSeal
            | StackedDRG8MiBPoSt
            | StackedDRG8MiBWindowPoSt
            | StackedDRG8MiBWinningPoSt => SectorSize::_8MiB,
            StackedDRG512MiBSeal
            | StackedDRG512MiBPoSt
            | StackedDRG512MiBWindowPoSt
            | StackedDRG512MiBWinningPoSt => SectorSize::_512MiB,
        }
    }

    /// Returns the partition size, in sectors, associated with a proof type.
    /// The partition size is the number of sectors proven in a single PoSt proof.
    pub fn window_post_partitions_sector(self) -> u64 {
        // Resolve to seal proof and then compute size from that.
        use RegisteredProof::*;
        match self {
            StackedDRG64GiBSeal | StackedDRG64GiBWindowPoSt | StackedDRG64GiBWinningPoSt => 2300,
            StackedDRG32GiBSeal
            | StackedDRG32GiBPoSt
            | StackedDRG32GiBWindowPoSt
            | StackedDRG32GiBWinningPoSt => 2349,
            StackedDRG2KiBSeal
            | StackedDRG2KiBPoSt
            | StackedDRG2KiBWindowPoSt
            | StackedDRG2KiBWinningPoSt => 2,
            StackedDRG8MiBSeal
            | StackedDRG8MiBPoSt
            | StackedDRG8MiBWindowPoSt
            | StackedDRG8MiBWinningPoSt => 2,
            StackedDRG512MiBSeal
            | StackedDRG512MiBPoSt
            | StackedDRG512MiBWindowPoSt
            | StackedDRG512MiBWinningPoSt => 2,
        }
    }

    /// Produces the winning PoSt-specific RegisteredProof corresponding
    /// to the receiving RegisteredProof.
    pub fn registered_winning_post_proof(self) -> Result<RegisteredProof, String> {
        use RegisteredProof::*;
        match self {
            StackedDRG64GiBSeal | StackedDRG64GiBWindowPoSt => Ok(StackedDRG64GiBWindowPoSt),
            StackedDRG32GiBSeal | StackedDRG32GiBWinningPoSt => Ok(StackedDRG32GiBWinningPoSt),
            StackedDRG2KiBSeal | StackedDRG2KiBWinningPoSt => Ok(StackedDRG2KiBWinningPoSt),
            StackedDRG8MiBSeal | StackedDRG8MiBWinningPoSt => Ok(StackedDRG8MiBWinningPoSt),
            StackedDRG512MiBSeal | StackedDRG512MiBWinningPoSt => Ok(StackedDRG512MiBWinningPoSt),
            _ => Err(format!(
                "Unsupported mapping from {:?} to PoSt-winning RegisteredProof",
                self
            )),
        }
    }

    /// Produces the windowed PoSt-specific RegisteredProof corresponding
    /// to the receiving RegisteredProof.
    pub fn registered_window_post_proof(self) -> Result<RegisteredProof, String> {
        use RegisteredProof::*;
        match self {
            StackedDRG64GiBSeal | StackedDRG64GiBWindowPoSt => Ok(StackedDRG64GiBWindowPoSt),
            StackedDRG32GiBSeal | StackedDRG32GiBWindowPoSt => Ok(StackedDRG32GiBWindowPoSt),
            StackedDRG2KiBSeal | StackedDRG2KiBWindowPoSt => Ok(StackedDRG2KiBWindowPoSt),
            StackedDRG8MiBSeal | StackedDRG8MiBWindowPoSt => Ok(StackedDRG8MiBWindowPoSt),
            StackedDRG512MiBSeal | StackedDRG512MiBWindowPoSt => Ok(StackedDRG512MiBWindowPoSt),
            _ => Err(format!(
                "Unsupported mapping from {:?} to PoSt-window RegisteredProof",
                self
            )),
        }
    }

    /// RegisteredSealProof produces the seal-specific RegisteredProof corresponding
    /// to the receiving RegisteredProof.
    pub fn registered_seal_proof(self) -> RegisteredProof {
        use RegisteredProof::*;
        match self {
            StackedDRG64GiBSeal | StackedDRG64GiBWindowPoSt | StackedDRG64GiBWinningPoSt => {
                StackedDRG64GiBSeal
            }
            StackedDRG32GiBSeal
            | StackedDRG32GiBPoSt
            | StackedDRG32GiBWindowPoSt
            | StackedDRG32GiBWinningPoSt => StackedDRG32GiBSeal,
            StackedDRG2KiBSeal
            | StackedDRG2KiBPoSt
            | StackedDRG2KiBWindowPoSt
            | StackedDRG2KiBWinningPoSt => StackedDRG2KiBSeal,
            StackedDRG8MiBSeal
            | StackedDRG8MiBPoSt
            | StackedDRG8MiBWindowPoSt
            | StackedDRG8MiBWinningPoSt => StackedDRG8MiBSeal,
            StackedDRG512MiBSeal
            | StackedDRG512MiBPoSt
            | StackedDRG512MiBWindowPoSt
            | StackedDRG512MiBWinningPoSt => StackedDRG512MiBSeal,
        }
    }
}

impl Default for RegisteredProof {
    fn default() -> Self {
        RegisteredProof::StackedDRG2KiBPoSt
    }
}

impl From<RegisteredProof> for RegisteredSealProof {
    fn from(p: RegisteredProof) -> Self {
        use RegisteredProof::*;

        match p {
            StackedDRG64GiBSeal | StackedDRG64GiBWindowPoSt | StackedDRG64GiBWinningPoSt => {
                RegisteredSealProof::StackedDrg64GiBV1
            }
            StackedDRG32GiBSeal
            | StackedDRG32GiBPoSt
            | StackedDRG32GiBWindowPoSt
            | StackedDRG32GiBWinningPoSt => RegisteredSealProof::StackedDrg32GiBV1,
            StackedDRG2KiBSeal
            | StackedDRG2KiBPoSt
            | StackedDRG2KiBWindowPoSt
            | StackedDRG2KiBWinningPoSt => RegisteredSealProof::StackedDrg2KiBV1,
            StackedDRG8MiBSeal
            | StackedDRG8MiBPoSt
            | StackedDRG8MiBWindowPoSt
            | StackedDRG8MiBWinningPoSt => RegisteredSealProof::StackedDrg8MiBV1,
            StackedDRG512MiBSeal
            | StackedDRG512MiBPoSt
            | StackedDRG512MiBWindowPoSt
            | StackedDRG512MiBWinningPoSt => RegisteredSealProof::StackedDrg512MiBV1,
        }
    }
}

impl From<RegisteredProof> for RegisteredPoStProof {
    fn from(p: RegisteredProof) -> Self {
        use RegisteredProof::*;

        match p {
            StackedDRG64GiBSeal | StackedDRG64GiBWindowPoSt | StackedDRG64GiBWinningPoSt => {
                RegisteredPoStProof::StackedDrgWindow64GiBV1
            }
            StackedDRG32GiBSeal
            | StackedDRG32GiBPoSt
            | StackedDRG32GiBWindowPoSt
            | StackedDRG32GiBWinningPoSt => RegisteredPoStProof::StackedDrgWindow32GiBV1,
            StackedDRG2KiBSeal
            | StackedDRG2KiBPoSt
            | StackedDRG2KiBWindowPoSt
            | StackedDRG2KiBWinningPoSt => RegisteredPoStProof::StackedDrgWindow2KiBV1,
            StackedDRG8MiBSeal
            | StackedDRG8MiBPoSt
            | StackedDRG8MiBWindowPoSt
            | StackedDRG8MiBWinningPoSt => RegisteredPoStProof::StackedDrgWindow8MiBV1,
            StackedDRG512MiBSeal
            | StackedDRG512MiBPoSt
            | StackedDRG512MiBWindowPoSt
            | StackedDRG512MiBWinningPoSt => RegisteredPoStProof::StackedDrgWindow512MiBV1,
        }
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
