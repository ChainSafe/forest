// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod post;
mod registered_proof;
mod seal;

pub use self::post::*;
pub use self::registered_proof::*;
pub use self::seal::*;

use crate::ActorID;
use encoding::{repr::*, tuple::*};
use num_bigint::BigInt;
use num_derive::FromPrimitive;
use std::fmt;

/// SectorNumber is a numeric identifier for a sector. It is usually relative to a miner.
pub type SectorNumber = u64;

/// The maximum assignable sector number.
/// Raising this would require modifying our AMT implementation.
pub const MAX_SECTOR_NUMBER: SectorNumber = i64::MAX as u64;

/// Unit of storage power (measured in bytes)
pub type StoragePower = BigInt;

/// The unit of spacetime committed to the network
pub type Spacetime = BigInt;

/// Unit of sector quality
pub type SectorQuality = BigInt;

/// SectorSize indicates one of a set of possible sizes in the network.
pub use fvm_shared::sector::SectorSize;

/// Sector ID which contains the sector number and the actor ID for the miner.
pub use fvm_shared::sector::SectorID;
