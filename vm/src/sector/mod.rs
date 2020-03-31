// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod post;
mod registered_proof;
mod seal;
mod serde;

pub use self::post::*;
pub use self::registered_proof::*;
pub use self::seal::*;

use crate::ActorID;
use ::serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use num_bigint::BigInt;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use std::fmt;

pub type SectorNumber = u64;

/// Unit of storage power (measured in bytes)
pub type StoragePower = BigInt;

/// SectorSize indicates one of a set of possible sizes in the network.
#[repr(u64)]
#[derive(Debug, FromPrimitive, Clone, Copy)]
pub enum SectorSize {
    _2KiB = 2 << 10,
    _8MiB = 8 << 20,
    _512MiB = 512 << 20,
    _32GiB = 32 << 30,
}

impl Serialize for SectorSize {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (*self as u64).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SectorSize {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v: u64 = Deserialize::deserialize(deserializer)?;
        FromPrimitive::from_u64(v)
            .ok_or_else(|| de::Error::custom(format!("Invalid sector size: {}", v)))
    }
}

impl fmt::Display for SectorSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

/// Sector ID which contains the sector number and the actor ID for the miner.
#[derive(Debug, Default, PartialEq)]
pub struct SectorID {
    pub miner: ActorID,
    pub number: SectorNumber,
}
