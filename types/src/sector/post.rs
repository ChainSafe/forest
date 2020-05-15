// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{RegisteredProof, SectorNumber};
use cid::Cid;
use vm::{ActorID, Randomness};

pub type PoStRandomness = Randomness;

/// Information about a sector necessary for PoSt verification
#[derive(Debug, PartialEq, Default, Clone, Eq)]
pub struct SectorInfo {
    /// Used when sealing - needs to be mapped to PoSt registered proof when used to verify a PoSt
    pub proof: RegisteredProof,
    pub sector_number: SectorNumber,
    pub sealed_cid: Cid,
}

// TODO docs
#[derive(Debug, PartialEq, Default, Clone, Eq)]
pub struct PoStProof {
    pub registered_proof: RegisteredProof,
    // TODO revisit if can be array in future
    pub proof_bytes: Vec<u8>,
}

/// Information needed to verify a Winning PoSt attached to a block header.
/// Note: this is not used within the state machine, but by the consensus/election mechanisms.
#[derive(Debug, PartialEq, Default, Clone, Eq)]
pub struct WinningPoStVerifyInfo {
    pub randomness: PoStRandomness,
    pub proofs: Vec<PoStProof>,
    pub challenge_sectors: Vec<SectorInfo>,
    /// Used to derive 32-byte prover ID
    pub prover: ActorID,
}

/// Information needed to verify a Window PoSt submitted directly to a miner actor.
#[derive(Debug, PartialEq, Default, Clone, Eq)]
pub struct WindowPoStVerifyInfo {
    pub randomness: PoStRandomness,
    pub proofs: Vec<PoStProof>,
    pub private_proof: Vec<SectorInfo>,
    pub prover: ActorID,
}

/// Information submitted by a miner to provide a Window PoSt.
#[derive(Debug, PartialEq, Default, Clone, Eq)]
pub struct OnChainWindowPoStVerifyInfo {
    pub proofs: Vec<PoStProof>,
}
