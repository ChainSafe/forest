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
use num_bigint::BigInt;
use std::fmt;

pub type SectorNumber = u64;

/// Unit of storage power (measured in bytes)
pub type StoragePower = BigInt;

/// SectorSize indicates one of a set of possible sizes in the network.
#[repr(u64)]
pub enum SectorSize {
    _2KiB = 2 << 10,
    _8MiB = 8 << 20,
    _512MiB = 512 << 20,
    _32GiB = 32 << 30,
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
