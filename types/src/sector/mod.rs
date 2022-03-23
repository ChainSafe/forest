// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod post;
pub use self::post::*;

pub use fvm_shared::sector::{
    AggregateSealVerifyInfo, AggregateSealVerifyProofAndInfos, InteractiveSealRandomness,
    OnChainWindowPoStVerifyInfo, PoStProof, PoStRandomness, RegisteredAggregateProof,
    RegisteredPoStProof, RegisteredSealProof, SealRandomness, SealVerifyInfo, SealVerifyParams,
    SectorID, SectorInfo, SectorSize, WindowPoStVerifyInfo, WinningPoStVerifyInfo,
};
use num_bigint::BigInt;
pub const RANDOMNESS_LENGTH: usize = 32;

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
