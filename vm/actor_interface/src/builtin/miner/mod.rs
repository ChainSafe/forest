// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use encoding::BytesDe;
use fil_types::{deadlines::DeadlineInfo, RegisteredSealProof, SectorNumber, SectorSize};
use forest_bitfield::BitField;
use ipld_blockstore::BlockStore;
use libp2p::PeerId;
use num_bigint::{bigint_ser, BigInt};
use serde::Serialize;
use std::borrow::Cow;
use std::error::Error;
use vm::{ActorState, DealID, TokenAmount};

/// Miner actor method.
pub type Method = actorv2::miner::Method;

/// Miner actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    V0(actorv0::miner::State),
    V2(actorv2::miner::State),
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> Result<Option<State>, Box<dyn Error>>
    where
        BS: BlockStore,
    {
        if actor.code == *actorv0::MINER_ACTOR_CODE_ID {
            Ok(store.get(&actor.state)?.map(State::V0))
        } else if actor.code == *actorv2::MINER_ACTOR_CODE_ID {
            Ok(store.get(&actor.state)?.map(State::V2))
        } else {
            Err(format!("Unknown actor code {}", actor.code).into())
        }
    }

    pub fn info<BS: BlockStore>(&self, store: &BS) -> Result<MinerInfo, Box<dyn Error>> {
        match self {
            State::V0(st) => {
                let info = st.get_info(store)?;

                let peer_id = PeerId::from_bytes(info.peer_id)
                    .map_err(|e| format!("bytes {:?} cannot be converted into a PeerId", e))?;

                Ok(MinerInfo {
                    owner: info.owner,
                    worker: info.worker,
                    control_addresses: info.control_addresses,
                    new_worker: info.pending_worker_key.as_ref().map(|k| k.new_worker),
                    worker_change_epoch: info
                        .pending_worker_key
                        .map(|k| k.effective_at)
                        .unwrap_or(-1),
                    peer_id,
                    multiaddrs: info.multi_address,
                    seal_proof_type: info.seal_proof_type,
                    sector_size: info.sector_size,
                    window_post_partition_sectors: info.window_post_partition_sectors,
                    consensus_fault_elapsed: -1,
                })
            }
            State::V2(st) => {
                let info = st.get_info(store)?;

                let peer_id = PeerId::from_bytes(info.peer_id)
                    .map_err(|e| format!("bytes {:?} cannot be converted into a PeerId", e))?;

                Ok(MinerInfo {
                    owner: info.owner,
                    worker: info.worker,
                    control_addresses: info.control_addresses,
                    new_worker: info.pending_worker_key.as_ref().map(|k| k.new_worker),
                    worker_change_epoch: info
                        .pending_worker_key
                        .map(|k| k.effective_at)
                        .unwrap_or(-1),
                    peer_id,
                    multiaddrs: info.multi_address,
                    seal_proof_type: info.seal_proof_type,
                    sector_size: info.sector_size,
                    window_post_partition_sectors: info.window_post_partition_sectors,
                    // TODO update on v2 update
                    consensus_fault_elapsed: -1,
                })
            }
        }
    }

    /// Loads deadlines for a miner's state
    pub fn for_each_deadline<BS: BlockStore>(
        &self,
        store: &BS,
        mut f: impl FnMut(u64, Deadline) -> Result<(), Box<dyn Error>>,
    ) -> Result<(), Box<dyn Error>> {
        match self {
            State::V0(st) => st
                .load_deadlines(store)?
                .for_each(store, |idx, dl| f(idx, Deadline::V0(dl))),
            State::V2(st) => st
                .load_deadlines(store)?
                .for_each(store, |idx, dl| f(idx, Deadline::V2(dl))),
        }
    }

    /// Loads deadline at index for a miner's state
    pub fn load_deadline<BS: BlockStore>(
        &self,
        store: &BS,
        idx: u64,
    ) -> Result<Deadline, Box<dyn Error>> {
        match self {
            State::V0(st) => Ok(st
                .load_deadlines(store)?
                .load_deadline(store, idx)
                .map(Deadline::V0)?),
            State::V2(st) => Ok(st
                .load_deadlines(store)?
                .load_deadline(store, idx)
                .map(Deadline::V2)?),
        }
    }

    /// Loads deadline at index for a miner's state
    pub fn deadline_info(&self, epoch: ChainEpoch) -> DeadlineInfo {
        match self {
            State::V0(st) => st.deadline_info(epoch),
            State::V2(st) => st.deadline_info(epoch),
        }
    }
}

/// Static information about miner
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MinerInfo {
    pub owner: Address,
    pub worker: Address,
    #[serde(with = "address::json::opt")]
    pub new_worker: Option<Address>,
    #[serde(with = "address::json::vec")]
    pub control_addresses: Vec<Address>, // Must all be ID addresses.
    pub worker_change_epoch: ChainEpoch,
    #[serde(with = "peer_id_json")]
    pub peer_id: PeerId,
    pub multiaddrs: Vec<BytesDe>,
    pub seal_proof_type: RegisteredSealProof,
    pub sector_size: SectorSize,
    pub window_post_partition_sectors: u64,
    pub consensus_fault_elapsed: ChainEpoch,
}

/// Deadline holds the state for all sectors due at a specific deadline.
pub enum Deadline {
    V0(actorv0::miner::Deadline),
    V2(actorv2::miner::Deadline),
}

impl Deadline {
    /// Consume state to return the deadline post submissions
    pub fn into_post_submissions(self) -> BitField {
        match self {
            Deadline::V0(dl) => dl.post_submissions,
            Deadline::V2(dl) => dl.post_submissions,
        }
    }

    /// For each partition of the deadline
    pub fn for_each<BS: BlockStore>(
        &self,
        store: &BS,
        mut f: impl FnMut(u64, Partition) -> Result<(), Box<dyn Error>>,
    ) -> Result<(), Box<dyn Error>> {
        match self {
            Deadline::V0(dl) => dl.for_each(store, |idx, part| {
                f(idx, Partition::V0(Cow::Borrowed(part)))
            }),
            Deadline::V2(dl) => dl.for_each(store, |idx, part| {
                f(idx, Partition::V2(Cow::Borrowed(part)))
            }),
        }
    }
}

pub enum Partition<'a> {
    V0(Cow<'a, actorv0::miner::Partition>),
    V2(Cow<'a, actorv2::miner::Partition>),
}

impl Partition<'_> {
    pub fn all_sectors(&self) -> &BitField {
        match self {
            Partition::V0(dl) => &dl.sectors,
            Partition::V2(dl) => &dl.sectors,
        }
    }
    pub fn faulty_sectors(&self) -> &BitField {
        match self {
            Partition::V0(dl) => &dl.faults,
            Partition::V2(dl) => &dl.faults,
        }
    }
    pub fn recovering_sectors(&self) -> &BitField {
        match self {
            Partition::V0(dl) => &dl.recoveries,
            Partition::V2(dl) => &dl.recoveries,
        }
    }
    pub fn live_sectors(&self) -> BitField {
        match self {
            Partition::V0(dl) => dl.live_sectors(),
            Partition::V2(dl) => dl.live_sectors(),
        }
    }
    pub fn active_sectors(&self) -> BitField {
        match self {
            Partition::V0(dl) => dl.active_sectors(),
            Partition::V2(dl) => dl.active_sectors(),
        }
    }
}

mod peer_id_json {
    use super::*;
    use serde::Serializer;

    pub fn serialize<S>(m: &PeerId, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // TODO Go impl seems to not have a valid output for this -- check
        m.to_string().serialize(serializer)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SectorOnChainInfo {
    pub sector_number: SectorNumber,
    /// The seal proof type implies the PoSt proofs
    pub seal_proof: RegisteredSealProof,
    /// CommR
    #[serde(with = "cid::json")]
    pub sealed_cid: Cid,
    pub deal_ids: Vec<DealID>,
    /// Epoch during which the sector proof was accepted
    pub activation: ChainEpoch,
    /// Epoch during which the sector expires
    pub expiration: ChainEpoch,
    /// Integral of active deals over sector lifetime
    #[serde(with = "bigint_ser::json")]
    pub deal_weight: BigInt,
    /// Integral of active verified deals over sector lifetime
    #[serde(with = "bigint_ser::json")]
    pub verified_deal_weight: BigInt,
    /// Pledge collected to commit this sector
    #[serde(with = "bigint_ser::json")]
    pub initial_pledge: TokenAmount,
    /// Expected one day projection of reward for sector computed at activation time
    #[serde(with = "bigint_ser::json")]
    pub expected_day_reward: TokenAmount,
    /// Expected twenty day projection of reward for sector computed at activation time
    #[serde(with = "bigint_ser::json")]
    pub expected_storage_pledge: TokenAmount,
}

impl From<actorv0::miner::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: actorv0::miner::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: info.seal_proof,
            sealed_cid: info.sealed_cid,
            deal_ids: info.deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge,
            expected_day_reward: info.expected_day_reward,
            expected_storage_pledge: info.expected_storage_pledge,
        }
    }
}

impl From<actorv2::miner::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: actorv2::miner::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: info.seal_proof,
            sealed_cid: info.sealed_cid,
            deal_ids: info.deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge,
            expected_day_reward: info.expected_day_reward,
            expected_storage_pledge: info.expected_storage_pledge,
        }
    }
}
