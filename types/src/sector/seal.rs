// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{RegisteredProof, SectorID, SectorNumber};
use cid::Cid;
use clock::ChainEpoch;
use encoding::{serde_bytes, tuple::*};
use vm::{DealID, Randomness};

pub type SealRandomness = Randomness;
pub type InteractiveSealRandomness = Randomness;

/// Information needed to verify a seal proof.
#[derive(Clone, Debug, PartialEq, Default, Serialize_tuple, Deserialize_tuple)]
pub struct SealVerifyInfo {
    pub sector_id: SectorID,
    // TODO revisit issue to remove this: https://github.com/filecoin-project/specs-actors/issues/276
    pub on_chain: SealVerifyParams,
    pub randomness: SealRandomness,
    pub interactive_randomness: InteractiveSealRandomness,
    pub unsealed_cid: Cid,
}

/// SealVerifyParams is the structure of information that must be sent with
/// a message to commit a sector. Most of this information is not needed in the
/// state tree but will be verified in sm.CommitSector. See SealCommitment for
/// data stored on the state tree for each sector.
#[derive(Clone, Debug, PartialEq, Default, Serialize_tuple, Deserialize_tuple)]
pub struct SealVerifyParams {
    pub sealed_cid: Cid,
    pub interactive_epoch: ChainEpoch,
    pub registered_proof: RegisteredProof,
    #[serde(with = "serde_bytes")]
    pub proof: Vec<u8>,
    pub deal_ids: Vec<DealID>,
    pub sector_num: SectorNumber,
    pub seal_rand_epoch: ChainEpoch,
}
