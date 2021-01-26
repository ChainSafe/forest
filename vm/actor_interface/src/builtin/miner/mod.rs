// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use encoding::BytesDe;
use fil_types::{deadlines::DeadlineInfo, RegisteredSealProof, SectorNumber, SectorSize};
use forest_bitfield::BitField;
use forest_json_utils::go_vec_visitor;
use ipld_blockstore::BlockStore;
use libp2p::PeerId;
use num_bigint::{bigint_ser, BigInt};
use serde::{Deserialize, Serialize};
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
    pub fn load<BS>(store: &BS, actor: &ActorState) -> Result<State, Box<dyn Error>>
    where
        BS: BlockStore,
    {
        if actor.code == *actorv0::MINER_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V0)
                .ok_or("Actor state doesn't exist in store")?)
        } else if actor.code == *actorv2::MINER_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V2)
                .ok_or("Actor state doesn't exist in store")?)
        } else {
            Err(format!("Unknown actor code {}", actor.code).into())
        }
    }

    pub fn info<BS: BlockStore>(&self, store: &BS) -> Result<MinerInfo, Box<dyn Error>> {
        match self {
            State::V0(st) => {
                let info = st.get_info(store)?;

                // Deserialize into peer id if valid, `None` if not.
                let peer_id = PeerId::from_bytes(info.peer_id).ok();

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

                // Deserialize into peer id if valid, `None` if not.
                let peer_id = PeerId::from_bytes(info.peer_id).ok();

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
                    consensus_fault_elapsed: info.consensus_fault_elapsed,
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

    /// Loads sectors corresponding to the bitfield. If no bitfield is passed in, return all.
    pub fn load_sectors<BS: BlockStore>(
        &self,
        store: &BS,
        sectors: Option<&BitField>,
    ) -> Result<Vec<SectorOnChainInfo>, Box<dyn Error>> {
        match self {
            State::V0(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = actorv0::miner::Sectors::load(store, &st.sectors)?;
                    let mut infos = Vec::with_capacity(sectors.amt.count() as usize);
                    sectors.amt.for_each(|_, info| {
                        infos.push(SectorOnChainInfo::from(info.clone()));
                        Ok(())
                    })?;
                    Ok(infos)
                }
            }
            State::V2(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = actorv2::miner::Sectors::load(store, &st.sectors)?;
                    let mut infos = Vec::with_capacity(sectors.amt.count() as usize);
                    sectors.amt.for_each(|_, info| {
                        infos.push(SectorOnChainInfo::from(info.clone()));
                        Ok(())
                    })?;
                    Ok(infos)
                }
            }
        }
    }

    /// Gets pre committed on chain info
    pub fn get_precommitted_sector<BS: BlockStore>(
        &self,
        store: &BS,
        sector_num: SectorNumber,
    ) -> Result<Option<SectorPreCommitOnChainInfo>, Box<dyn Error>> {
        match self {
            State::V0(st) => Ok(st
                .get_precommitted_sector(store, sector_num)?
                .map(From::from)),
            State::V2(st) => Ok(st
                .get_precommitted_sector(store, sector_num)?
                .map(From::from)),
        }
    }

    /// Loads a specific sector number
    pub fn get_sector<BS: BlockStore>(
        &self,
        store: &BS,
        sector_num: u64,
    ) -> Result<Option<SectorOnChainInfo>, Box<dyn Error>> {
        match self {
            State::V0(st) => Ok(st.get_sector(store, sector_num)?.map(From::from)),
            State::V2(st) => Ok(st.get_sector(store, sector_num)?.map(From::from)),
        }
    }

    /// Loads deadline at index for a miner's state
    pub fn deadline_info(&self, epoch: ChainEpoch) -> DeadlineInfo {
        match self {
            State::V0(st) => st.deadline_info(epoch),
            State::V2(st) => st.deadline_info(epoch),
        }
    }

    /// Gets fee debt of miner state
    pub fn fee_debt(&self) -> TokenAmount {
        match self {
            State::V0(_) => TokenAmount::from(0),
            State::V2(st) => st.fee_debt.clone(),
        }
    }

    /// Number of post period deadlines.
    pub fn num_deadlines(&self) -> u64 {
        match self {
            State::V0(_) => actorv0::miner::WPOST_PERIOD_DEADLINES,
            State::V2(_) => actorv2::miner::WPOST_PERIOD_DEADLINES,
        }
    }
}

/// Static information about miner
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MinerInfo {
    #[serde(with = "address::json")]
    pub owner: Address,
    #[serde(with = "address::json")]
    pub worker: Address,
    #[serde(with = "address::json::opt")]
    pub new_worker: Option<Address>,
    #[serde(with = "address::json::vec")]
    pub control_addresses: Vec<Address>, // Must all be ID addresses.
    pub worker_change_epoch: ChainEpoch,
    #[serde(with = "peer_id_json")]
    pub peer_id: Option<PeerId>,
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

#[allow(clippy::large_enum_variant)]
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

    pub fn serialize<S>(m: &Option<PeerId>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        m.as_ref().map(|pid| pid.to_string()).serialize(serializer)
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

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SectorPreCommitOnChainInfo {
    pub info: SectorPreCommitInfo,
    #[serde(with = "bigint_ser::json")]
    pub pre_commit_deposit: TokenAmount,
    pub pre_commit_epoch: ChainEpoch,
    /// Integral of active deals over sector lifetime, 0 if CommittedCapacity sector
    #[serde(with = "bigint_ser::json")]
    pub deal_weight: BigInt,
    /// Integral of active verified deals over sector lifetime
    #[serde(with = "bigint_ser::json")]
    pub verified_deal_weight: BigInt,
}

impl From<actorv0::miner::SectorPreCommitOnChainInfo> for SectorPreCommitOnChainInfo {
    fn from(spc: actorv0::miner::SectorPreCommitOnChainInfo) -> Self {
        Self {
            info: spc.info.into(),
            pre_commit_deposit: spc.pre_commit_deposit,
            pre_commit_epoch: spc.pre_commit_epoch,
            deal_weight: spc.deal_weight,
            verified_deal_weight: spc.verified_deal_weight,
        }
    }
}

impl From<actorv2::miner::SectorPreCommitOnChainInfo> for SectorPreCommitOnChainInfo {
    fn from(spc: actorv2::miner::SectorPreCommitOnChainInfo) -> Self {
        Self {
            info: spc.info.into(),
            pre_commit_deposit: spc.pre_commit_deposit,
            pre_commit_epoch: spc.pre_commit_epoch,
            deal_weight: spc.deal_weight,
            verified_deal_weight: spc.verified_deal_weight,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SectorPreCommitInfo {
    pub seal_proof: RegisteredSealProof,
    pub sector_number: SectorNumber,
    /// CommR
    #[serde(with = "cid::json", rename = "SealedCID")]
    pub sealed_cid: Cid,
    pub seal_rand_epoch: ChainEpoch,
    #[serde(with = "go_vec_visitor", rename = "DealIDs")]
    pub deal_ids: Vec<DealID>,
    pub expiration: ChainEpoch,
    /// Whether to replace a "committed capacity" no-deal sector (requires non-empty DealIDs)
    pub replace_capacity: bool,
    /// The committed capacity sector to replace, and its deadline/partition location
    pub replace_sector_deadline: u64,
    pub replace_sector_partition: u64,
    pub replace_sector_number: SectorNumber,
}

impl From<actorv0::miner::SectorPreCommitInfo> for SectorPreCommitInfo {
    fn from(spc: actorv0::miner::SectorPreCommitInfo) -> Self {
        Self {
            seal_proof: spc.seal_proof,
            sector_number: spc.sector_number,
            sealed_cid: spc.sealed_cid,
            seal_rand_epoch: spc.seal_rand_epoch,
            deal_ids: spc.deal_ids,
            expiration: spc.expiration,
            replace_capacity: spc.replace_capacity,
            replace_sector_deadline: spc.replace_sector_deadline,
            replace_sector_partition: spc.replace_sector_partition,
            replace_sector_number: spc.replace_sector_number,
        }
    }
}

impl From<actorv2::miner::SectorPreCommitInfo> for SectorPreCommitInfo {
    fn from(spc: actorv2::miner::SectorPreCommitInfo) -> Self {
        Self {
            seal_proof: spc.seal_proof,
            sector_number: spc.sector_number,
            sealed_cid: spc.sealed_cid,
            seal_rand_epoch: spc.seal_rand_epoch,
            deal_ids: spc.deal_ids,
            expiration: spc.expiration,
            replace_capacity: spc.replace_capacity,
            replace_sector_deadline: spc.replace_sector_deadline,
            replace_sector_partition: spc.replace_sector_partition,
            replace_sector_number: spc.replace_sector_number,
        }
    }
}
