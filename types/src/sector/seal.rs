// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    ActorID, Randomness, RegisteredAggregateProof, RegisteredSealProof, SectorID, SectorNumber,
};
use cid::Cid;
use clock::ChainEpoch;
use encoding::{serde_bytes, tuple::*};
use vm::DealID;

/// Randomness used for Seal proofs.
pub type SealRandomness = Randomness;

/// Randomness used when verifying a seal proof. This is just a seed value.
pub type InteractiveSealRandomness = Randomness;

/// Information needed to verify a seal proof.
#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct SealVerifyInfo {
    pub registered_proof: RegisteredSealProof,
    pub sector_id: SectorID,
    pub deal_ids: Vec<DealID>,
    pub randomness: SealRandomness,
    pub interactive_randomness: InteractiveSealRandomness,
    #[serde(with = "serde_bytes")]
    pub proof: Vec<u8>,
    pub sealed_cid: Cid,   // Commr
    pub unsealed_cid: Cid, // Commd
}

/// SealVerifyParams is the structure of information that must be sent with
/// a message to commit a sector. Most of this information is not needed in the
/// state tree but will be verified in sm.CommitSector. See SealCommitment for
/// data stored on the state tree for each sector.
#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct SealVerifyParams {
    pub sealed_cid: Cid,
    pub interactive_epoch: ChainEpoch,
    pub registered_seal_proof: RegisteredSealProof,
    #[serde(with = "serde_bytes")]
    pub proof: Vec<u8>,
    pub deal_ids: Vec<DealID>,
    pub sector_num: SectorNumber,
    pub seal_rand_epoch: ChainEpoch,
}

/// Information needed to verify an aggregated seal proof.
#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct AggregateSealVerifyInfo {
    pub sector_number: SectorNumber,
    pub randomness: SealRandomness,
    pub interactive_randomness: InteractiveSealRandomness,

    pub sealed_cid: Cid,   // Commr
    pub unsealed_cid: Cid, // Commd
}

#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct AggregateSealVerifyProofAndInfos {
    pub miner: ActorID,
    pub seal_proof: RegisteredSealProof,
    pub aggregate_proof: RegisteredAggregateProof,
    #[serde(with = "serde_bytes")]
    pub proof: Vec<u8>,
    pub infos: Vec<AggregateSealVerifyInfo>,
}
