// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use encoding::repr::Serialize_repr;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use std::convert::TryFrom;
use std::fmt::{self, Display, Formatter};

/// Specifies the network version
#[derive(Debug, PartialEq, Clone, Copy, PartialOrd, Serialize_repr, FromPrimitive)]
#[repr(u32)]
pub enum NetworkVersion {
    /// genesis (specs-actors v0.9.3)
    V0,
    /// breeze (specs-actors v0.9.7)
    V1,
    /// smoke (specs-actors v0.9.8)
    V2,
    /// ignition (specs-actors v0.9.11)
    V3,
    /// actors v2 (specs-actors v2.0.3)
    V4,
    /// tape (specs-actors v2.1.0)
    V5,
    /// kumquat (specs-actors v2.2.0)
    V6,
    /// calico (specs-actors v2.3.2)
    V7,
    /// persian (post-2.3.2 behaviour transition)
    V8,
    /// orange (post-2.3.2 behaviour transition)
    V9,
    /// trust (specs-actors v3.0.1)
    V10,
    /// norwegian (specs-actors v3.1.0)
    V11,
    /// turbo (specs-actors v4.0.0)
    V12,
    /// hyperdrive (specs-actors v5.0.1)
    V13,
    /// chocolate (specs-actors v6.0.0)
    V14,
}

impl TryFrom<u32> for NetworkVersion {
    type Error = ();

    fn try_from(v: u32) -> Result<Self, Self::Error> {
        match FromPrimitive::from_u32(v) {
            Some(nv) => Ok(nv),
            _ => Err(()),
        }
    }
}

impl Display for NetworkVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::V0 => write!(f, "V0"),
            Self::V1 => write!(f, "V1"),
            Self::V2 => write!(f, "V2"),
            Self::V3 => write!(f, "V3"),
            Self::V4 => write!(f, "V4"),
            Self::V5 => write!(f, "V5"),
            Self::V6 => write!(f, "V6"),
            Self::V7 => write!(f, "V7"),
            Self::V8 => write!(f, "V8"),
            Self::V9 => write!(f, "V9"),
            Self::V10 => write!(f, "V10"),
            Self::V11 => write!(f, "V11"),
            Self::V12 => write!(f, "V12"),
            Self::V13 => write!(f, "V13"),
            Self::V14 => write!(f, "V14"),
        }
    }
}
