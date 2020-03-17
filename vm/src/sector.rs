// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

// This ordering, defines mappings to UInt in a way which MUST never change.
#[derive(PartialEq, Eq, Copy, Clone, FromPrimitive, Debug, Hash)]
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
}

pub struct SealVerifyInfo {
    // TODO implement SealVerifyInfo
}

pub struct PoStVerifyInfo {
    // TODO implement PoStVerifyInfo
}
