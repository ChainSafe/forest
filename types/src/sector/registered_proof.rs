// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::SectorSize;
use crate::NetworkVersion;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[cfg(feature = "proofs")]
use std::convert::TryFrom;

#[derive(PartialEq, Eq, Copy, Clone, Debug, Hash)]
pub enum RegisteredSealProof {
    StackedDRG2KiBV1,
    StackedDRG512MiBV1,
    StackedDRG8MiBV1,
    StackedDRG32GiBV1,
    StackedDRG64GiBV1,

    StackedDRG2KiBV1P1,
    StackedDRG512MiBV1P1,
    StackedDRG8MiBV1P1,
    StackedDRG32GiBV1P1,
    StackedDRG64GiBV1P1,
    Invalid(i64),
}

impl RegisteredSealProof {
    /// Returns registered seal proof for given sector size
    pub fn from_sector_size(size: SectorSize, network_version: NetworkVersion) -> Self {
        if network_version < NetworkVersion::V7 {
            match size {
                SectorSize::_2KiB => Self::StackedDRG2KiBV1,
                SectorSize::_8MiB => Self::StackedDRG8MiBV1,
                SectorSize::_512MiB => Self::StackedDRG512MiBV1,
                SectorSize::_32GiB => Self::StackedDRG32GiBV1,
                SectorSize::_64GiB => Self::StackedDRG64GiBV1,
            }
        } else {
            match size {
                SectorSize::_2KiB => Self::StackedDRG2KiBV1P1,
                SectorSize::_8MiB => Self::StackedDRG8MiBV1P1,
                SectorSize::_512MiB => Self::StackedDRG512MiBV1P1,
                SectorSize::_32GiB => Self::StackedDRG32GiBV1P1,
                SectorSize::_64GiB => Self::StackedDRG64GiBV1P1,
            }
        }
    }

    pub fn update_to_v1(&mut self) {
        *self = match self {
            Self::StackedDRG2KiBV1 => Self::StackedDRG2KiBV1P1,
            Self::StackedDRG512MiBV1 => Self::StackedDRG512MiBV1P1,
            Self::StackedDRG8MiBV1 => Self::StackedDRG8MiBV1P1,
            Self::StackedDRG32GiBV1 => Self::StackedDRG32GiBV1P1,
            Self::StackedDRG64GiBV1 => Self::StackedDRG64GiBV1P1,
            _ => return,
        };
    }

    #[deprecated(since = "0.1.10", note = "Logic should exist in actors")]
    /// The maximum duration a sector sealed with this proof may exist between activation and expiration.
    pub fn sector_maximum_lifetime(self) -> clock::ChainEpoch {
        // For all Stacked DRG sectors, the max is 5 years
        let epochs_per_year = 1_262_277;
        5 * epochs_per_year
    }
}

#[derive(PartialEq, Eq, Copy, Clone, Debug, Hash)]
pub enum RegisteredPoStProof {
    StackedDRGWinning2KiBV1,
    StackedDRGWinning8MiBV1,
    StackedDRGWinning512MiBV1,
    StackedDRGWinning32GiBV1,
    StackedDRGWinning64GiBV1,
    StackedDRGWindow2KiBV1,
    StackedDRGWindow8MiBV1,
    StackedDRGWindow512MiBV1,
    StackedDRGWindow32GiBV1,
    StackedDRGWindow64GiBV1,
    Invalid(i64),
}

impl RegisteredPoStProof {
    /// Returns the sector size of the proof type, which is measured in bytes.
    pub fn sector_size(self) -> Result<SectorSize, String> {
        use RegisteredPoStProof::*;
        match self {
            StackedDRGWindow2KiBV1 | StackedDRGWinning2KiBV1 => Ok(SectorSize::_2KiB),
            StackedDRGWindow8MiBV1 | StackedDRGWinning8MiBV1 => Ok(SectorSize::_8MiB),
            StackedDRGWindow512MiBV1 | StackedDRGWinning512MiBV1 => Ok(SectorSize::_512MiB),
            StackedDRGWindow32GiBV1 | StackedDRGWinning32GiBV1 => Ok(SectorSize::_32GiB),
            StackedDRGWindow64GiBV1 | StackedDRGWinning64GiBV1 => Ok(SectorSize::_64GiB),
            Invalid(i) => Err(format!("unsupported proof type: {}", i)),
        }
    }

    /// RegisteredSealProof produces the seal-specific RegisteredProof corresponding
    /// to the receiving RegisteredProof.
    pub fn registered_seal_proof(self) -> Result<RegisteredSealProof, String> {
        use RegisteredPoStProof::*;
        match self {
            StackedDRGWindow64GiBV1 | StackedDRGWinning64GiBV1 => {
                Ok(RegisteredSealProof::StackedDRG64GiBV1)
            }
            StackedDRGWindow32GiBV1 | StackedDRGWinning32GiBV1 => {
                Ok(RegisteredSealProof::StackedDRG32GiBV1)
            }
            StackedDRGWindow2KiBV1 | StackedDRGWinning2KiBV1 => {
                Ok(RegisteredSealProof::StackedDRG2KiBV1)
            }
            StackedDRGWindow8MiBV1 | StackedDRGWinning8MiBV1 => {
                Ok(RegisteredSealProof::StackedDRG8MiBV1)
            }
            StackedDRGWindow512MiBV1 | StackedDRGWinning512MiBV1 => {
                Ok(RegisteredSealProof::StackedDRG512MiBV1)
            }
            Invalid(i) => Err(format!("unsupported proof type: {}", i)),
        }
    }
}

impl RegisteredSealProof {
    /// Returns the sector size of the proof type, which is measured in bytes.
    pub fn sector_size(self) -> Result<SectorSize, String> {
        use RegisteredSealProof::*;
        match self {
            StackedDRG2KiBV1 | StackedDRG2KiBV1P1 => Ok(SectorSize::_2KiB),
            StackedDRG8MiBV1 | StackedDRG8MiBV1P1 => Ok(SectorSize::_8MiB),
            StackedDRG512MiBV1 | StackedDRG512MiBV1P1 => Ok(SectorSize::_512MiB),
            StackedDRG32GiBV1 | StackedDRG32GiBV1P1 => Ok(SectorSize::_32GiB),
            StackedDRG64GiBV1 | StackedDRG64GiBV1P1 => Ok(SectorSize::_64GiB),
            Invalid(i) => Err(format!("unsupported proof type: {}", i)),
        }
    }

    /// Returns the partition size, in sectors, associated with a proof type.
    /// The partition size is the number of sectors proven in a single PoSt proof.
    pub fn window_post_partitions_sector(self) -> Result<u64, String> {
        // Resolve to seal proof and then compute size from that.
        use RegisteredSealProof::*;
        match self {
            StackedDRG64GiBV1 | StackedDRG64GiBV1P1 => Ok(2300),
            StackedDRG32GiBV1 | StackedDRG32GiBV1P1 => Ok(2349),
            StackedDRG2KiBV1 | StackedDRG2KiBV1P1 => Ok(2),
            StackedDRG8MiBV1 | StackedDRG8MiBV1P1 => Ok(2),
            StackedDRG512MiBV1 | StackedDRG512MiBV1P1 => Ok(2),
            Invalid(i) => Err(format!("unsupported proof type: {}", i)),
        }
    }

    /// Produces the winning PoSt-specific RegisteredProof corresponding
    /// to the receiving RegisteredProof.
    pub fn registered_winning_post_proof(self) -> Result<RegisteredPoStProof, String> {
        use RegisteredPoStProof::*;
        match self {
            Self::StackedDRG64GiBV1 | Self::StackedDRG64GiBV1P1 => Ok(StackedDRGWinning64GiBV1),
            Self::StackedDRG32GiBV1 | Self::StackedDRG32GiBV1P1 => Ok(StackedDRGWinning32GiBV1),
            Self::StackedDRG2KiBV1 | Self::StackedDRG2KiBV1P1 => Ok(StackedDRGWinning2KiBV1),
            Self::StackedDRG8MiBV1 | Self::StackedDRG8MiBV1P1 => Ok(StackedDRGWinning8MiBV1),
            Self::StackedDRG512MiBV1 | Self::StackedDRG512MiBV1P1 => Ok(StackedDRGWinning512MiBV1),
            Self::Invalid(_) => Err(format!(
                "Unsupported mapping from {:?} to PoSt-winning RegisteredProof",
                self
            )),
        }
    }

    /// Produces the windowed PoSt-specific RegisteredProof corresponding
    /// to the receiving RegisteredProof.
    pub fn registered_window_post_proof(self) -> Result<RegisteredPoStProof, String> {
        use RegisteredPoStProof::*;
        match self {
            Self::StackedDRG64GiBV1 | Self::StackedDRG64GiBV1P1 => Ok(StackedDRGWindow64GiBV1),
            Self::StackedDRG32GiBV1 | Self::StackedDRG32GiBV1P1 => Ok(StackedDRGWindow32GiBV1),
            Self::StackedDRG2KiBV1 | Self::StackedDRG2KiBV1P1 => Ok(StackedDRGWindow2KiBV1),
            Self::StackedDRG8MiBV1 | Self::StackedDRG8MiBV1P1 => Ok(StackedDRGWindow8MiBV1),
            Self::StackedDRG512MiBV1 | Self::StackedDRG512MiBV1P1 => Ok(StackedDRGWindow512MiBV1),
            Self::Invalid(_) => Err(format!(
                "Unsupported mapping from {:?} to PoSt-window RegisteredProof",
                self
            )),
        }
    }
}

macro_rules! i64_conversion {
    ($ty:ident; $( $var:ident => $val:expr, )*) => {
        impl From<i64> for $ty {
            fn from(value: i64) -> Self {
                match value {
                    $( $val => $ty::$var, )*
                    other => $ty::Invalid(other),
                }
            }
        }
        impl From<$ty> for i64 {
            fn from(proof: $ty) -> Self {
                match proof {
                    $( $ty::$var => $val, )*
                    $ty::Invalid(other) => other,
                }
            }
        }
    }
}

i64_conversion! {
    RegisteredPoStProof;
    StackedDRGWinning2KiBV1 => 0,
    StackedDRGWinning8MiBV1 => 1,
    StackedDRGWinning512MiBV1 => 2,
    StackedDRGWinning32GiBV1 => 3,
    StackedDRGWinning64GiBV1 => 4,
    StackedDRGWindow2KiBV1 => 5,
    StackedDRGWindow8MiBV1 => 6,
    StackedDRGWindow512MiBV1 => 7,
    StackedDRGWindow32GiBV1 => 8,
    StackedDRGWindow64GiBV1 => 9,
}

i64_conversion! {
    RegisteredSealProof;
    StackedDRG2KiBV1 => 0,
    StackedDRG8MiBV1 => 1,
    StackedDRG512MiBV1 => 2,
    StackedDRG32GiBV1 => 3,
    StackedDRG64GiBV1 => 4,

    StackedDRG2KiBV1P1 => 5,
    StackedDRG8MiBV1P1 => 6,
    StackedDRG512MiBV1P1 => 7,
    StackedDRG32GiBV1P1 => 8,
    StackedDRG64GiBV1P1 => 9,
}

#[cfg(feature = "proofs")]
impl TryFrom<RegisteredSealProof> for filecoin_proofs_api::RegisteredSealProof {
    type Error = String;
    fn try_from(p: RegisteredSealProof) -> Result<Self, Self::Error> {
        use RegisteredSealProof::*;
        match p {
            StackedDRG64GiBV1 => Ok(Self::StackedDrg64GiBV1),
            StackedDRG32GiBV1 => Ok(Self::StackedDrg32GiBV1),
            StackedDRG2KiBV1 => Ok(Self::StackedDrg2KiBV1),
            StackedDRG8MiBV1 => Ok(Self::StackedDrg8MiBV1),
            StackedDRG512MiBV1 => Ok(Self::StackedDrg512MiBV1),
            StackedDRG64GiBV1P1 => Ok(Self::StackedDrg64GiBV1_1),
            StackedDRG32GiBV1P1 => Ok(Self::StackedDrg32GiBV1_1),
            StackedDRG2KiBV1P1 => Ok(Self::StackedDrg2KiBV1_1),
            StackedDRG8MiBV1P1 => Ok(Self::StackedDrg8MiBV1_1),
            StackedDRG512MiBV1P1 => Ok(Self::StackedDrg512MiBV1_1),
            Invalid(i) => Err(format!("unsupported proof type: {}", i)),
        }
    }
}

#[cfg(feature = "proofs")]
impl TryFrom<RegisteredPoStProof> for filecoin_proofs_api::RegisteredPoStProof {
    type Error = String;
    fn try_from(p: RegisteredPoStProof) -> Result<Self, Self::Error> {
        use RegisteredPoStProof::*;
        match p {
            StackedDRGWinning2KiBV1 => Ok(Self::StackedDrgWinning2KiBV1),
            StackedDRGWinning8MiBV1 => Ok(Self::StackedDrgWinning8MiBV1),
            StackedDRGWinning512MiBV1 => Ok(Self::StackedDrgWinning512MiBV1),
            StackedDRGWinning32GiBV1 => Ok(Self::StackedDrgWinning32GiBV1),
            StackedDRGWinning64GiBV1 => Ok(Self::StackedDrgWinning64GiBV1),
            StackedDRGWindow2KiBV1 => Ok(Self::StackedDrgWindow2KiBV1),
            StackedDRGWindow8MiBV1 => Ok(Self::StackedDrgWindow8MiBV1),
            StackedDRGWindow512MiBV1 => Ok(Self::StackedDrgWindow512MiBV1),
            StackedDRGWindow32GiBV1 => Ok(Self::StackedDrgWindow32GiBV1),
            StackedDRGWindow64GiBV1 => Ok(Self::StackedDrgWindow64GiBV1),
            Invalid(i) => Err(format!("unsupported proof type: {}", i)),
        }
    }
}

impl Serialize for RegisteredPoStProof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        i64::from(*self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RegisteredPoStProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let val = i64::deserialize(deserializer)?;
        Ok(Self::from(val))
    }
}

impl Serialize for RegisteredSealProof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        i64::from(*self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RegisteredSealProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let val = i64::deserialize(deserializer)?;
        Ok(Self::from(val))
    }
}
