// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{DealID, Randomness, RegisteredProof, SectorID, SectorNumber};
use cid::Cid;
use clock::ChainEpoch;

pub type SealRandomness = Randomness;
pub type InteractiveSealRandomness = Randomness;

/// Information needed to verify a seal proof.
#[derive(Debug, PartialEq, Default)]
pub struct SealVerifyInfo {
    pub sector_id: SectorID,
    // TODO revisit issue to remove this: https://github.com/filecoin-project/specs-actors/issues/276
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
