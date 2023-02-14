// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::ops::Deref;

use fvm_shared::sector::{
    RegisteredPoStProof as RegisteredPoStProofV2, RegisteredSealProof as RegisteredSealProofV2,
    SectorInfo as SectorInfoV2, SectorSize as SectorSizeV2,
};
use fvm_shared3::sector::{
    PoStProof as PoStProofV3, RegisteredPoStProof as RegisteredPoStProofV3,
    RegisteredSealProof as RegisteredSealProofV3, SectorInfo as SectorInfoV3,
    SectorSize as SectorSizeV3,
};

use crate::{version::NetworkVersion, Inner};

/// Represents a shim over RegisteredSealProof from fvm_shared with convenience
/// methods to convert to an older version of the type
///
/// # Examples
/// ```
/// 
/// // Create FVM2 RegisteredSealProof normally
/// let fvm2_proof = fvm_shared::sector::RegisteredSealProof::StackedDRG2KiBV1;
///
/// // Create a correspndoning FVM3 RegisteredSealProof
/// let fvm3_proof = fvm_shared3::sector::RegisteredSealProof::StackedDRG2KiBV1;
///
/// // Create a shim out of fvm2 proof, ensure conversions are correct
/// let proof_shim = forest_shim::sector::RegisteredSealProof::from(fvm2_proof);
/// assert_eq!(fvm3_proof, *proof_shim);
/// assert_eq!(fvm2_proof, proof_shim.into());
/// ```
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy)]
pub struct RegisteredSealProof(RegisteredSealProofV3);

impl RegisteredSealProof {
    pub fn from_sector_size(size: SectorSize, network_version: NetworkVersion) -> Self {
        RegisteredSealProof(RegisteredSealProofV3::from_sector_size(
            *size,
            network_version.into(),
        ))
    }
}

impl From<RegisteredSealProofV3> for RegisteredSealProof {
    fn from(value: RegisteredSealProofV3) -> Self {
        RegisteredSealProof(value)
    }
}

impl crate::Inner for RegisteredSealProof {
    type FVM = RegisteredSealProofV3;
}

impl Deref for RegisteredSealProof {
    type Target = RegisteredSealProofV3;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<RegisteredSealProofV2> for RegisteredSealProof {
    fn from(value: RegisteredSealProofV2) -> RegisteredSealProof {
        let proof = match value {
            RegisteredSealProofV2::StackedDRG2KiBV1 => RegisteredSealProofV3::StackedDRG2KiBV1,
            RegisteredSealProofV2::StackedDRG512MiBV1 => RegisteredSealProofV3::StackedDRG512MiBV1,
            RegisteredSealProofV2::StackedDRG8MiBV1 => RegisteredSealProofV3::StackedDRG8MiBV1,
            RegisteredSealProofV2::StackedDRG32GiBV1 => RegisteredSealProofV3::StackedDRG32GiBV1,
            RegisteredSealProofV2::StackedDRG64GiBV1 => RegisteredSealProofV3::StackedDRG64GiBV1,
            RegisteredSealProofV2::StackedDRG2KiBV1P1 => RegisteredSealProofV3::StackedDRG2KiBV1P1,
            RegisteredSealProofV2::StackedDRG512MiBV1P1 => {
                RegisteredSealProofV3::StackedDRG512MiBV1P1
            }
            RegisteredSealProofV2::StackedDRG8MiBV1P1 => RegisteredSealProofV3::StackedDRG8MiBV1P1,
            RegisteredSealProofV2::StackedDRG32GiBV1P1 => {
                RegisteredSealProofV3::StackedDRG32GiBV1P1
            }
            RegisteredSealProofV2::StackedDRG64GiBV1P1 => {
                RegisteredSealProofV3::StackedDRG64GiBV1P1
            }
            RegisteredSealProofV2::Invalid(i64) => RegisteredSealProofV3::Invalid(i64),
        };
        RegisteredSealProof(proof)
    }
}

impl From<RegisteredSealProof> for RegisteredSealProofV2 {
    fn from(value: RegisteredSealProof) -> RegisteredSealProofV2 {
        match value.0 {
            RegisteredSealProofV3::StackedDRG2KiBV1 => RegisteredSealProofV2::StackedDRG2KiBV1,
            RegisteredSealProofV3::StackedDRG512MiBV1 => RegisteredSealProofV2::StackedDRG512MiBV1,
            RegisteredSealProofV3::StackedDRG8MiBV1 => RegisteredSealProofV2::StackedDRG8MiBV1,
            RegisteredSealProofV3::StackedDRG32GiBV1 => RegisteredSealProofV2::StackedDRG32GiBV1,
            RegisteredSealProofV3::StackedDRG64GiBV1 => RegisteredSealProofV2::StackedDRG64GiBV1,
            RegisteredSealProofV3::StackedDRG2KiBV1P1 => RegisteredSealProofV2::StackedDRG2KiBV1P1,
            RegisteredSealProofV3::StackedDRG512MiBV1P1 => {
                RegisteredSealProofV2::StackedDRG512MiBV1P1
            }
            RegisteredSealProofV3::StackedDRG8MiBV1P1 => RegisteredSealProofV2::StackedDRG8MiBV1P1,
            RegisteredSealProofV3::StackedDRG32GiBV1P1 => {
                RegisteredSealProofV2::StackedDRG32GiBV1P1
            }
            RegisteredSealProofV3::StackedDRG64GiBV1P1 => {
                RegisteredSealProofV2::StackedDRG64GiBV1P1
            }
            RegisteredSealProofV3::Invalid(i64) => RegisteredSealProofV2::Invalid(i64),
        }
    }
}

/// Represents a shim over SectorInfo from fvm_shared with convenience methods
/// to convert to an older version of the type
pub struct SectorInfo(SectorInfoV3);

impl From<SectorInfoV3> for SectorInfo {
    fn from(value: SectorInfoV3) -> Self {
        SectorInfo(value)
    }
}

impl SectorInfo {
    pub fn new(
        proof: RegisteredSealProofV3,
        sector_number: fvm_shared3::sector::SectorNumber,
        sealed_cid: cid::Cid,
    ) -> Self {
        SectorInfo(SectorInfoV3 {
            proof,
            sector_number,
            sealed_cid,
        })
    }
}

impl Deref for SectorInfo {
    type Target = SectorInfoV3;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<SectorInfo> for SectorInfoV2 {
    fn from(value: SectorInfo) -> SectorInfoV2 {
        SectorInfoV2 {
            proof: RegisteredSealProof(value.0.proof).into(),
            sealed_cid: value.sealed_cid,
            sector_number: value.sector_number,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct RegisteredPoStProof(RegisteredPoStProofV3);

impl Deref for RegisteredPoStProof {
    type Target = RegisteredPoStProofV3;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<RegisteredPoStProofV3> for RegisteredPoStProof {
    fn from(value: RegisteredPoStProofV3) -> Self {
        RegisteredPoStProof(value)
    }
}

impl Inner for RegisteredPoStProof {
    type FVM = RegisteredPoStProofV3;
}

impl From<RegisteredPoStProofV2> for RegisteredPoStProof {
    fn from(value: RegisteredPoStProofV2) -> RegisteredPoStProof {
        let num_id: i64 = value.into();
        RegisteredPoStProof(RegisteredPoStProofV3::from(num_id))
    }
}

/// SectorSize indicates one of a set of possible sizes in the network.
#[derive(Clone, Debug, PartialEq, Copy, serde::Serialize)]
#[serde(transparent)]
pub struct SectorSize(SectorSizeV3);

impl From<SectorSizeV3> for SectorSize {
    fn from(value: SectorSizeV3) -> Self {
        SectorSize(value)
    }
}

impl Deref for SectorSize {
    type Target = SectorSizeV3;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Inner for SectorSize {
    type FVM = SectorSizeV3;
}

impl From<SectorSizeV2> for SectorSize {
    fn from(value: SectorSizeV2) -> SectorSize {
        let size = match value {
            SectorSizeV2::_2KiB => SectorSizeV3::_2KiB,
            SectorSizeV2::_8MiB => SectorSizeV3::_8MiB,
            SectorSizeV2::_512MiB => SectorSizeV3::_512MiB,
            SectorSizeV2::_32GiB => SectorSizeV3::_32GiB,
            SectorSizeV2::_64GiB => SectorSizeV3::_64GiB,
        };

        SectorSize(size)
    }
}

impl From<SectorSize> for SectorSizeV2 {
    fn from(value: SectorSize) -> SectorSizeV2 {
        match value.0 {
            SectorSizeV3::_2KiB => SectorSizeV2::_2KiB,
            SectorSizeV3::_8MiB => SectorSizeV2::_8MiB,
            SectorSizeV3::_512MiB => SectorSizeV2::_512MiB,
            SectorSizeV3::_32GiB => SectorSizeV2::_32GiB,
            SectorSizeV3::_64GiB => SectorSizeV2::_64GiB,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct PoStProof(PoStProofV3);

impl PoStProof {
    pub fn new(reg_post_proof: RegisteredPoStProof, proof_bytes: Vec<u8>) -> Self {
        PoStProof(PoStProofV3 {
            post_proof: *reg_post_proof,
            proof_bytes,
        })
    }
}

impl Deref for PoStProof {
    type Target = PoStProofV3;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
