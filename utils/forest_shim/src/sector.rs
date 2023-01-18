use fvm_ipld_encoding::repr::*;
use fvm_ipld_encoding::tuple::*;
use fvm_shared::sector::SectorSize as SectorSize_v2;
use fvm_shared3::sector::{
    PoStProof as PoStProof_v3, RegisteredPoStProof as RegisteredPoStProof_v3,
    RegisteredSealProof as RegisteredSealProof_v3, SectorInfo as SectorInfo_v3,
    SectorSize as SectorSize_v3,
};
use num_derive::FromPrimitive;
use serde::{Deserialize, Serialize};

pub type PoStProof = PoStProof_v3;
pub type RegisteredPoStProof = RegisteredPoStProof_v3;
pub type RegisteredSealProof = RegisteredSealProof_v3;
pub type SectorInfo = SectorInfo_v3;

#[derive(Clone, Debug, PartialEq, Eq, Copy, FromPrimitive, Serialize, Deserialize)]
#[repr(transparent)]
pub struct SectorSize(SectorSize_v3);

impl From<SectorSize_v3> for SectorSize {
    fn from(other: SectorSize_v3) -> Self {
        todo!()
    }
}

impl From<SectorSize_v2> for SectorSize {
    fn from(other: SectorSize_v2) -> Self {
        todo!()
    }
}

impl From<SectorSize> for SectorSize_v3 {
    fn from(other: SectorSize) -> Self {
        other.0
    }
}

impl From<SectorSize> for SectorSize_v2 {
    fn from(other: SectorSize) -> Self {
        todo!()
    }
}
