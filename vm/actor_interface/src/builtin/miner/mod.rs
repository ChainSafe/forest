// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use cid::multihash::MultihashDigest;
use cid::Cid;
use encoding::BytesDe;
use fil_types::{
    deadlines::DeadlineInfo, RegisteredPoStProof, RegisteredSealProof, SectorNumber, SectorSize,
};
use forest_json_utils::go_vec_visitor;
use fvm_ipld_bitfield::BitField;
use fvm_shared::bigint::BigInt;
use fvm_shared::clock::ChainEpoch;
use ipld_blockstore::BlockStore;
use libp2p::PeerId;
use num_bigint::bigint_ser::json;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use vm::{ActorState, DealID, TokenAmount};

use anyhow::Context;

use crate::power::Claim;
/// Miner actor method.
pub type Method = fil_actor_miner_v7::Method;

/// Miner actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    V7(fil_actor_miner_v7::State),
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> anyhow::Result<State>
    where
        BS: BlockStore,
    {
        if actor.code == Cid::new_v1(cid::RAW, cid::Code::Identity.digest(b"fil/7/storageminer")) {
            Ok(store
                .get_anyhow(&actor.state)?
                .map(State::V7)
                .context("Actor state doesn't exist in store")?)
        } else {
            Err(anyhow::anyhow!("Unknown miner actor code {}", actor.code))
        }
    }

    pub fn info<BS: BlockStore>(&self, store: &BS) -> anyhow::Result<MinerInfo> {
        match self {
            State::V7(st) => {
                let fvm_store = ipld_blockstore::FvmRefStore::new(store);
                let info = st.get_info(&fvm_store)?;

                // Deserialize into peer id if valid, `None` if not.
                let peer_id = PeerId::from_bytes(&info.peer_id).ok();

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
                    window_post_proof_type: info.window_post_proof_type,
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
        mut f: impl FnMut(u64, Deadline) -> Result<(), anyhow::Error>,
    ) -> anyhow::Result<()> {
        match self {
            State::V7(st) => {
                let fvm_store = ipld_blockstore::FvmRefStore::new(store);
                st.load_deadlines(&fvm_store)?
                    .for_each(&Default::default(), &fvm_store, |idx, dl| {
                        f(idx as u64, Deadline::V7(dl)).expect("FIXME");
                        Ok(())
                    })
                    .expect("FIXME");
                Ok(())
            }
        }
    }

    /// Loads deadline at index for a miner's state
    pub fn load_deadline<BS: BlockStore>(
        &self,
        _store: &BS,
        _idx: u64,
    ) -> anyhow::Result<Deadline> {
        unimplemented!()
    }

    /// Loads sectors corresponding to the bitfield. If no bitfield is passed in, return all.
    pub fn load_sectors<BS: BlockStore>(
        &self,
        store: &BS,
        sectors: Option<&BitField>,
    ) -> anyhow::Result<Vec<SectorOnChainInfo>> {
        match self {
            State::V7(st) => {
                let fvm_store = ipld_blockstore::FvmRefStore::new(store);
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&fvm_store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_v7::Sectors::load(&fvm_store, &st.sectors)?;
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
        _store: &BS,
        _sector_num: SectorNumber,
    ) -> anyhow::Result<Option<SectorPreCommitOnChainInfo>> {
        unimplemented!()
    }

    /// Loads a specific sector number
    pub fn get_sector<BS: BlockStore>(
        &self,
        _store: &BS,
        _sector_num: u64,
    ) -> anyhow::Result<Option<SectorOnChainInfo>> {
        unimplemented!()
    }

    /// Loads deadline at index for a miner's state
    pub fn deadline_info(&self, _epoch: ChainEpoch) -> DeadlineInfo {
        match self {
            State::V7(_st) => todo!(),
        }
    }

    /// Gets fee debt of miner state
    pub fn fee_debt(&self) -> TokenAmount {
        match self {
            State::V7(st) => st.fee_debt.clone(),
        }
    }

    /// Number of post period deadlines.
    pub fn num_deadlines(&self) -> u64 {
        match self {
            State::V7(_) => {
                fil_actors_runtime_v7::runtime::Policy::default().wpost_period_deadlines
            }
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
    pub window_post_proof_type: RegisteredPoStProof,
    pub sector_size: SectorSize,
    pub window_post_partition_sectors: u64,
    pub consensus_fault_elapsed: ChainEpoch,
}

impl MinerInfo {
    pub fn worker(&self) -> Address {
        self.worker
    }

    pub fn sector_size(&self) -> SectorSize {
        self.sector_size
    }
}

#[derive(Serialize, Deserialize)]
pub struct MinerPower {
    pub miner_power: Claim,
    pub total_power: Claim,
    pub has_min_power: bool,
}

/// Deadline holds the state for all sectors due at a specific deadline.
pub enum Deadline {
    V7(fil_actor_miner_v7::Deadline),
}

impl Deadline {
    /// For each partition of the deadline
    pub fn for_each<BS: BlockStore>(
        &self,
        store: &BS,
        mut f: impl FnMut(u64, Partition) -> Result<(), anyhow::Error>,
    ) -> anyhow::Result<()> {
        match self {
            Deadline::V7(dl) => {
                let fvm_store = ipld_blockstore::FvmRefStore::new(store);
                dl.for_each(&fvm_store, |idx, part| {
                    f(idx as u64, Partition::V7(Cow::Borrowed(part))).expect("FIXME");
                    Ok(())
                })
                .expect("FIXME");
                Ok(())
            }
        }
    }

    pub fn disputable_proof_count<BS: BlockStore>(&self, store: &BS) -> anyhow::Result<usize> {
        Ok(match self {
            Deadline::V7(dl) => {
                let fvm_store = ipld_blockstore::FvmRefStore::new(store);
                dl.optimistic_proofs_snapshot_amt(&fvm_store)?
                    .count()
                    .try_into()
                    .unwrap()
            }
        })
    }

    pub fn partitions_posted(&self) -> &BitField {
        match self {
            Deadline::V7(_dl) => todo!(), // &dl.partitions_posted.into(),
        }
    }
}

#[allow(clippy::large_enum_variant)]
pub enum Partition<'a> {
    V7(Cow<'a, fil_actor_miner_v7::Partition>),
}

impl Partition<'_> {
    pub fn all_sectors(&self) -> &BitField {
        match self {
            Partition::V7(_dl) => todo!(),
        }
    }
    pub fn faulty_sectors(&self) -> &BitField {
        match self {
            Partition::V7(_dl) => todo!(),
        }
    }
    pub fn recovering_sectors(&self) -> &BitField {
        match self {
            Partition::V7(_dl) => todo!(),
        }
    }
    pub fn live_sectors(&self) -> BitField {
        match self {
            Partition::V7(dl) => dl.live_sectors(),
        }
    }
    pub fn active_sectors(&self) -> BitField {
        match self {
            Partition::V7(dl) => dl.active_sectors(),
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
    #[serde(with = "json")]
    pub deal_weight: BigInt,
    /// Integral of active verified deals over sector lifetime
    #[serde(with = "json")]
    pub verified_deal_weight: BigInt,
    /// Pledge collected to commit this sector
    #[serde(with = "json")]
    pub initial_pledge: TokenAmount,
    /// Expected one day projection of reward for sector computed at activation time
    #[serde(with = "json")]
    pub expected_day_reward: TokenAmount,
    /// Expected twenty day projection of reward for sector computed at activation time
    #[serde(with = "json")]
    pub expected_storage_pledge: TokenAmount,
}

impl From<fil_actor_miner_v7::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: fil_actor_miner_v7::SectorOnChainInfo) -> Self {
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
    #[serde(with = "json")]
    pub pre_commit_deposit: TokenAmount,
    pub pre_commit_epoch: ChainEpoch,
    /// Integral of active deals over sector lifetime, 0 if CommittedCapacity sector
    #[serde(with = "json")]
    pub deal_weight: BigInt,
    /// Integral of active verified deals over sector lifetime
    #[serde(with = "json")]
    pub verified_deal_weight: BigInt,
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
