// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{ActorID, Randomness, RegisteredProof, SectorID, SectorNumber};
use cid::Cid;

// TODO check if this can be changed to an array
pub type ChallengeTicketsCommitment = Vec<u8>;
pub type PoStRandomness = Randomness;
pub type PartialTicket = [u8; 32];

/// PoStVerifyInfo is the structure of all the information a verifier
/// needs to verify a PoSt.
#[derive(Debug, PartialEq, Default)]
pub struct PoStVerifyInfo {
    pub randomness: PoStRandomness,
    pub candidates: Vec<PoStCandidate>,
    pub proofs: Vec<PoStProof>,
    pub eligible_sectors: Vec<SectorInfo>,
    pub prover: ActorID,
    pub challenge_count: u64,
}

// TODO docs
#[derive(Debug, PartialEq, Default)]
pub struct SectorInfo {
    /// Used when sealing - needs to be mapped to PoSt registered proof when used to verify a PoSt
    pub proof: RegisteredProof,
    pub sector_number: SectorNumber,
    pub sealed_cid: Cid,
}

// TODO docs
#[derive(Debug, PartialEq, Default)]
pub struct OnChainElectionPoStVerifyInfo {
    /// each PoStCandidate has its own RegisteredProof
    pub candidates: Vec<PoStCandidate>,
    /// each PoStProof has its own RegisteredProof
    pub proofs: Vec<PoStProof>,
}

// TODO docs
#[derive(Debug, PartialEq, Default)]
pub struct OnChainPoStVerifyInfo {
    /// each PoStCandidate has its own RegisteredProof
    pub candidates: Vec<PoStCandidate>,
    /// each PoStProof has its own RegisteredProof
    pub proofs: Vec<PoStProof>,
}

// TODO docs
#[derive(Debug, PartialEq, Default)]
pub struct PoStCandidate {
    pub registered_proof: RegisteredProof,
    pub ticket: PartialTicket,
    pub private_proof: PrivatePoStCandidateProof,
    pub sector_id: SectorID,
    pub challenge_index: usize,
}

// TODO docs
#[derive(Debug, PartialEq, Default)]
pub struct PoStProof {
    pub registered_proof: RegisteredProof,
    pub proof_bytes: Vec<u8>,
}

// TODO docs
#[derive(Debug, PartialEq, Default)]
pub struct PrivatePoStCandidateProof {
    pub registered_proof: RegisteredProof,
    pub externalized: Vec<u8>,
}
