// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::version::NetworkVersion;
use anyhow::bail;
use cid::Cid;
use fvm_ipld_encoding::repr::{Deserialize_repr, Serialize_repr};
use fvm_shared2::sector::{
    PoStProof as PoStProofV2, RegisteredPoStProof as RegisteredPoStProofV2,
    RegisteredSealProof as RegisteredSealProofV2, SectorInfo as SectorInfoV2,
    SectorSize as SectorSizeV2,
};
pub use fvm_shared3::sector::{
    RegisteredPoStProof as RegisteredPoStProofV3, RegisteredSealProof as RegisteredSealProofV3,
    SectorSize as SectorSizeV3, StoragePower,
};
pub use fvm_shared4::sector::{
    PoStProof as PoStProofV4, RegisteredPoStProof as RegisteredPoStProofV4,
    RegisteredSealProof as RegisteredSealProofV4, SectorInfo as SectorInfoV4,
    SectorSize as SectorSizeV4,
};
use get_size2::GetSize;
use num_derive::FromPrimitive;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

pub type SectorNumber = fvm_shared4::sector::SectorNumber;

/// Represents a shim over `RegisteredSealProof` from `fvm_shared` with
/// convenience methods to convert to an older version of the type
///
/// # Examples
/// ```
/// # use forest::doctest_private::RegisteredSealProof;
/// // Create FVM2 RegisteredSealProof normally
/// let fvm2_proof = fvm_shared2::sector::RegisteredSealProof::StackedDRG2KiBV1;
///
/// // Create a correspndoning FVM3 RegisteredSealProof
/// let fvm3_proof = fvm_shared3::sector::RegisteredSealProof::StackedDRG2KiBV1;
///
/// // Create a correspndoning FVM4 RegisteredSealProof
/// let fvm4_proof = fvm_shared4::sector::RegisteredSealProof::StackedDRG2KiBV1;
///
/// // Create a shim out of fvm2 proof, ensure conversions are correct
/// let proof_shim = RegisteredSealProof::from(fvm2_proof);
/// assert_eq!(fvm4_proof, *proof_shim);
/// assert_eq!(fvm3_proof, proof_shim.into());
/// assert_eq!(fvm2_proof, proof_shim.into());
/// ```
#[derive(
    serde::Serialize, serde::Deserialize, Clone, Copy, Eq, PartialEq, Debug, derive_more::Deref,
)]
pub struct RegisteredSealProof(RegisteredSealProofV4);

impl RegisteredSealProof {
    pub fn from_sector_size(size: SectorSize, network_version: NetworkVersion) -> Self {
        RegisteredSealProof(RegisteredSealProofV4::from_sector_size(
            size.into(),
            network_version.into(),
        ))
    }

    pub fn registered_winning_post_proof(self) -> anyhow::Result<RegisteredPoStProofV4> {
        use fvm_shared4::sector::RegisteredPoStProof as PoStProof;
        use fvm_shared4::sector::RegisteredSealProof as SealProof;
        match self.0 {
            SealProof::StackedDRG64GiBV1
            | SealProof::StackedDRG64GiBV1P1
            | SealProof::StackedDRG64GiBV1P1_Feat_SyntheticPoRep
            | SealProof::StackedDRG64GiBV1P2_Feat_NiPoRep => {
                Ok(PoStProof::StackedDRGWinning64GiBV1)
            }
            SealProof::StackedDRG32GiBV1
            | SealProof::StackedDRG32GiBV1P1
            | SealProof::StackedDRG32GiBV1P1_Feat_SyntheticPoRep
            | SealProof::StackedDRG32GiBV1P2_Feat_NiPoRep => {
                Ok(PoStProof::StackedDRGWinning32GiBV1)
            }
            SealProof::StackedDRG2KiBV1
            | SealProof::StackedDRG2KiBV1P1
            | SealProof::StackedDRG2KiBV1P1_Feat_SyntheticPoRep
            | SealProof::StackedDRG2KiBV1P2_Feat_NiPoRep => Ok(PoStProof::StackedDRGWinning2KiBV1),
            SealProof::StackedDRG8MiBV1
            | SealProof::StackedDRG8MiBV1P1
            | SealProof::StackedDRG8MiBV1P1_Feat_SyntheticPoRep
            | SealProof::StackedDRG8MiBV1P2_Feat_NiPoRep => Ok(PoStProof::StackedDRGWinning8MiBV1),
            SealProof::StackedDRG512MiBV1
            | SealProof::StackedDRG512MiBV1P1
            | SealProof::StackedDRG512MiBV1P1_Feat_SyntheticPoRep
            | SealProof::StackedDRG512MiBV1P2_Feat_NiPoRep => {
                Ok(PoStProof::StackedDRGWinning512MiBV1)
            }
            SealProof::Invalid(_) => bail!(
                "Unsupported mapping from {:?} to PoSt-winning RegisteredProof",
                self
            ),
        }
    }
}

impl From<i64> for RegisteredSealProof {
    fn from(value: i64) -> Self {
        RegisteredSealProof(RegisteredSealProofV4::from(value))
    }
}

macro_rules! registered_seal_proof_conversion {
    ($($internal:ty),+) => {
        $(
            impl From<$internal> for RegisteredSealProof {
                fn from(value: $internal) -> Self {
                    let num_id: i64 = value.into();
                    RegisteredSealProof::from(num_id)
                }
            }
            impl From<RegisteredSealProof> for $internal {
                fn from(value: RegisteredSealProof) -> $internal {
                    let num_id: i64 = value.0.into();
                    <$internal>::from(num_id)
                }
            }
        )+
    };
}

registered_seal_proof_conversion!(
    RegisteredSealProofV2,
    RegisteredSealProofV3,
    RegisteredSealProofV4
);

impl RegisteredSealProof {
    pub fn invalid() -> Self {
        RegisteredSealProof(RegisteredSealProofV4::Invalid(0))
    }
}

#[cfg(test)]
impl quickcheck::Arbitrary for RegisteredSealProof {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Self(i64::arbitrary(g).into())
    }
}

/// Represents a shim over `SectorInfo` from `fvm_shared` with convenience
/// methods to convert to an older version of the type
#[derive(
    Eq,
    PartialEq,
    Debug,
    Clone,
    derive_more::From,
    derive_more::Into,
    derive_more::Deref,
    Serialize,
    Deserialize,
)]
pub struct SectorInfo(SectorInfoV4);

#[cfg(test)]
impl quickcheck::Arbitrary for SectorInfo {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Self(SectorInfoV4 {
            proof: RegisteredSealProof::arbitrary(g).into(),
            sector_number: u64::arbitrary(g),
            sealed_cid: cid::Cid::arbitrary(g),
        })
    }
}

impl SectorInfo {
    pub fn new(
        proof: RegisteredSealProofV4,
        sector_number: SectorNumber,
        sealed_cid: cid::Cid,
    ) -> Self {
        SectorInfo(SectorInfoV4 {
            proof,
            sector_number,
            sealed_cid,
        })
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

/// Information about a sector necessary for PoSt verification
#[derive(
    Eq, PartialEq, Debug, Clone, derive_more::From, derive_more::Into, Serialize, Deserialize,
)]
pub struct ExtendedSectorInfo {
    pub proof: RegisteredSealProof,
    pub sector_number: SectorNumber,
    pub sector_key: Option<Cid>,
    pub sealed_cid: Cid,
}

impl From<&ExtendedSectorInfo> for SectorInfo {
    fn from(value: &ExtendedSectorInfo) -> SectorInfo {
        SectorInfo::new(value.proof.into(), value.sector_number, value.sealed_cid)
    }
}

#[cfg(test)]
impl quickcheck::Arbitrary for ExtendedSectorInfo {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Self {
            proof: RegisteredSealProof::arbitrary(g),
            sector_number: u64::arbitrary(g),
            sector_key: Option::<cid::Cid>::arbitrary(g),
            sealed_cid: cid::Cid::arbitrary(g),
        }
    }
}

#[derive(
    serde::Serialize,
    serde::Deserialize,
    Clone,
    Debug,
    Eq,
    PartialEq,
    derive_more::Into,
    derive_more::Deref,
)]
pub struct RegisteredPoStProof(RegisteredPoStProofV4);

#[cfg(test)]
impl quickcheck::Arbitrary for RegisteredPoStProof {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Self(RegisteredPoStProofV4::from(i64::arbitrary(g)))
    }
}

impl TryFrom<RegisteredPoStProof> for fil_actors_shared::filecoin_proofs_api::RegisteredPoStProof {
    type Error = anyhow::Error;

    fn try_from(value: RegisteredPoStProof) -> Result<Self, Self::Error> {
        value.0.try_into().map_err(|e: String| anyhow::anyhow!(e))
    }
}

impl From<i64> for RegisteredPoStProof {
    fn from(value: i64) -> Self {
        RegisteredPoStProof(RegisteredPoStProofV4::from(value))
    }
}

impl From<RegisteredPoStProofV2> for RegisteredPoStProof {
    fn from(value: RegisteredPoStProofV2) -> RegisteredPoStProof {
        let num_id: i64 = value.into();
        RegisteredPoStProof(RegisteredPoStProofV4::from(num_id))
    }
}

impl From<RegisteredPoStProofV3> for RegisteredPoStProof {
    fn from(value: RegisteredPoStProofV3) -> RegisteredPoStProof {
        let num_id: i64 = value.into();
        RegisteredPoStProof(RegisteredPoStProofV4::from(num_id))
    }
}

impl From<RegisteredPoStProofV4> for RegisteredPoStProof {
    fn from(value: RegisteredPoStProofV4) -> RegisteredPoStProof {
        RegisteredPoStProof(value)
    }
}

impl From<RegisteredPoStProof> for RegisteredPoStProofV3 {
    fn from(value: RegisteredPoStProof) -> RegisteredPoStProofV3 {
        let num_id: i64 = value.0.into();
        RegisteredPoStProofV3::from(num_id)
    }
}

impl From<RegisteredPoStProof> for RegisteredPoStProofV2 {
    fn from(value: RegisteredPoStProof) -> RegisteredPoStProofV2 {
        let num_id: i64 = value.0.into();
        RegisteredPoStProofV2::from(num_id)
    }
}

/// `SectorSize` indicates one of a set of possible sizes in the network.
#[derive(Clone, Debug, PartialEq, Eq, Copy, FromPrimitive, Serialize_repr, Deserialize_repr)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
#[repr(u64)]
pub enum SectorSize {
    _2KiB = 2 << 10,
    _8MiB = 8 << 20,
    _512MiB = 512 << 20,
    _32GiB = 32 << 30,
    _64GiB = 2 * (32 << 30),
}

macro_rules! sector_size_conversion {
    ($($internal:ty),+) => {
        $(
            impl From<$internal> for SectorSize {
                fn from(value: $internal) -> Self {
                    match value {
                        <$internal>::_2KiB => SectorSize::_2KiB,
                        <$internal>::_8MiB => SectorSize::_8MiB,
                        <$internal>::_512MiB => SectorSize::_512MiB,
                        <$internal>::_32GiB => SectorSize::_32GiB,
                        <$internal>::_64GiB => SectorSize::_64GiB,
                    }
                }
            }
            impl From<SectorSize> for $internal {
                fn from(value: SectorSize) -> $internal {
                    match value {
                        SectorSize::_2KiB => <$internal>::_2KiB,
                        SectorSize::_8MiB => <$internal>::_8MiB,
                        SectorSize::_512MiB => <$internal>::_512MiB,
                        SectorSize::_32GiB => <$internal>::_32GiB,
                        SectorSize::_64GiB => <$internal>::_64GiB,
                    }
                }
            }
        )+
    };
}

sector_size_conversion!(SectorSizeV2, SectorSizeV3, SectorSizeV4);

#[derive(
    serde::Serialize,
    serde::Deserialize,
    Clone,
    Debug,
    PartialEq,
    derive_more::From,
    derive_more::Into,
    derive_more::Deref,
    Eq,
)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub struct PoStProof(PoStProofV4);

impl Hash for PoStProof {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let PoStProofV4 {
            post_proof,
            proof_bytes,
        } = &self.0;
        post_proof.hash(state);
        proof_bytes.hash(state);
    }
}

impl PoStProof {
    pub fn new(reg_post_proof: RegisteredPoStProof, proof_bytes: Vec<u8>) -> Self {
        PoStProof(PoStProofV4 {
            post_proof: *reg_post_proof,
            proof_bytes,
        })
    }
}

impl From<PoStProofV2> for PoStProof {
    fn from(value: PoStProofV2) -> PoStProof {
        PoStProof(PoStProofV4 {
            post_proof: *RegisteredPoStProof::from(value.post_proof),
            proof_bytes: value.proof_bytes,
        })
    }
}

impl GetSize for PoStProof {
    fn get_heap_size(&self) -> usize {
        self.0.proof_bytes.get_heap_size()
    }
}

pub fn convert_window_post_proof_v1_to_v1p1(
    rpp: RegisteredPoStProofV3,
) -> anyhow::Result<RegisteredPoStProofV3> {
    match rpp {
        RegisteredPoStProofV3::StackedDRGWindow2KiBV1
        | RegisteredPoStProofV3::StackedDRGWindow2KiBV1P1 => {
            Ok(RegisteredPoStProofV3::StackedDRGWindow2KiBV1P1)
        }
        RegisteredPoStProofV3::StackedDRGWindow8MiBV1
        | RegisteredPoStProofV3::StackedDRGWindow8MiBV1P1 => {
            Ok(RegisteredPoStProofV3::StackedDRGWindow8MiBV1P1)
        }
        RegisteredPoStProofV3::StackedDRGWindow512MiBV1
        | RegisteredPoStProofV3::StackedDRGWindow512MiBV1P1 => {
            Ok(RegisteredPoStProofV3::StackedDRGWindow512MiBV1P1)
        }
        RegisteredPoStProofV3::StackedDRGWindow32GiBV1
        | RegisteredPoStProofV3::StackedDRGWindow32GiBV1P1 => {
            Ok(RegisteredPoStProofV3::StackedDRGWindow32GiBV1P1)
        }
        RegisteredPoStProofV3::StackedDRGWindow64GiBV1
        | RegisteredPoStProofV3::StackedDRGWindow64GiBV1P1 => {
            Ok(RegisteredPoStProofV3::StackedDRGWindow64GiBV1P1)
        }
        other => anyhow::bail!("Invalid proof type: {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sector_size_ser_deser() {
        let orig_sector_size = fvm_shared3::sector::SectorSize::_2KiB;
        let orig_json_repr = serde_json::to_string(&orig_sector_size).unwrap();

        let shimmed_sector_size = crate::shim::sector::SectorSize::_2KiB;
        let shimmed_json_repr = serde_json::to_string(&shimmed_sector_size).unwrap();

        assert_eq!(orig_json_repr, shimmed_json_repr);

        let shimmed_deser: crate::shim::sector::SectorSize =
            serde_json::from_str(&shimmed_json_repr).unwrap();
        let orig_deser: fvm_shared3::sector::SectorSize =
            serde_json::from_str(&orig_json_repr).unwrap();

        assert_eq!(shimmed_deser as u64, orig_deser as u64);
    }
}
