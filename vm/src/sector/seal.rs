// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{DealID, Randomness, RegisteredProof, SectorID, SectorNumber};
use cid::Cid;
use clock::ChainEpoch;

pub type SealRandomness = Randomness;
pub type InteractiveSealRandomness = Randomness;

/// SealVerifyInfo is the structure of all the information a verifier
/// needs to verify a Seal.
#[derive(Debug, PartialEq, Default)]
pub struct SealVerifyInfo {
    pub sector_id: SectorID,
    pub on_chain: OnChainSealVerifyInfo,
    pub randomness: SealRandomness,
    pub interactive_randomness: InteractiveSealRandomness,
    pub unsealed_cid: Cid,
}

/// OnChainSealVerifyInfo is the structure of information that must be sent with
/// a message to commit a sector. Most of this information is not needed in the
/// state tree but will be verified in sm.CommitSector. See SealCommitment for
/// data stored on the state tree for each sector.
#[derive(Debug, PartialEq, Default)]
pub struct OnChainSealVerifyInfo {
    pub sealed_cid: Cid,
    pub interactive_epoch: ChainEpoch,
    pub registered_proof: RegisteredProof,
    pub proof: Vec<u8>,
    pub deal_ids: Vec<DealID>,
    pub sector_num: SectorNumber,
    pub seal_rand_epoch: ChainEpoch,
}
