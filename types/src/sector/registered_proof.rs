// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::SectorSize;
use crate::NetworkVersion;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[cfg(feature = "proofs")]
use std::convert::TryFrom;

/// Seal proof type which defines the version and sector size.
pub use fvm_shared::sector::RegisteredSealProof;

/// Proof of spacetime type, indicating version and sector size of the proof.
pub use fvm_shared::sector::RegisteredPoStProof;

/// Seal proof type which defines the version and sector size.
#[derive(PartialEq, Eq, Copy, Clone, Debug, Hash)]
pub enum RegisteredAggregateProof {
    SnarkPackV1,
    Invalid(i64),
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
    RegisteredAggregateProof;
    SnarkPackV1 => 0,
}
#[cfg(feature = "proofs")]
impl TryFrom<RegisteredAggregateProof> for filecoin_proofs_api::RegisteredAggregationProof {
    type Error = String;
    fn try_from(p: RegisteredAggregateProof) -> Result<Self, Self::Error> {
        use RegisteredAggregateProof::*;
        match p {
            SnarkPackV1 => Ok(Self::SnarkPackV1),
            Invalid(i) => Err(format!("unsupported aggregate proof type: {}", i)),
        }
    }
}

impl Serialize for RegisteredAggregateProof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        i64::from(*self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RegisteredAggregateProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let val = i64::deserialize(deserializer)?;
        Ok(Self::from(val))
    }
}
