// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod ext;

use crate::{
    rpc::types::SectorPreCommitOnChainInfo,
    shim::{
        actors::{convert::*, power::Claim},
        address::Address,
        clock::ChainEpoch,
        deal::DealID,
        econ::TokenAmount,
        runtime::Policy,
        sector::SectorSize,
    },
    utils::db::CborStoreExt as _,
};
use cid::Cid;
use fil_actor_miner_state::v12::{BeneficiaryTerm, PendingBeneficiaryChange};
use fil_actor_miner_state::v17::VestingFunds as VestingFundsV17;
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{BytesDe, serde_bytes};
use fvm_shared2::{
    clock::QuantSpec,
    sector::{RegisteredPoStProof, RegisteredSealProof, SectorNumber},
};
use num::BigInt;
use serde::{Deserialize, Serialize};
use spire_enum::prelude::delegated_enum;
use std::borrow::{Borrow as _, Cow};

/// Miner actor method.
pub type Method = fil_actor_miner_state::v8::Method;

/// Miner actor state.
#[derive(Serialize, Debug)]
#[serde(untagged)]
#[delegated_enum(impl_conversions)]
pub enum State {
    // V7(fil_actor_miner_v7::State),
    V8(fil_actor_miner_state::v8::State),
    V9(fil_actor_miner_state::v9::State),
    V10(fil_actor_miner_state::v10::State),
    V11(fil_actor_miner_state::v11::State),
    V12(fil_actor_miner_state::v12::State),
    V13(fil_actor_miner_state::v13::State),
    V14(fil_actor_miner_state::v14::State),
    V15(fil_actor_miner_state::v15::State),
    V16(fil_actor_miner_state::v16::State),
    V17(fil_actor_miner_state::v17::State),
}

impl State {
    #[allow(clippy::too_many_arguments)]
    pub fn default_latest_version(
        info: Cid,
        pre_commit_deposits: fvm_shared4::econ::TokenAmount,
        locked_funds: fvm_shared4::econ::TokenAmount,
        vesting_funds: VestingFundsV17,
        fee_debt: fvm_shared4::econ::TokenAmount,
        initial_pledge: fvm_shared4::econ::TokenAmount,
        pre_committed_sectors: Cid,
        pre_committed_sectors_cleanup: Cid,
        allocated_sectors: Cid,
        sectors: Cid,
        proving_period_start: fvm_shared4::clock::ChainEpoch,
        current_deadline: u64,
        deadlines: Cid,
        early_terminations: BitField,
        deadline_cron_active: bool,
    ) -> Self {
        State::V17(fil_actor_miner_state::v17::State {
            info,
            pre_commit_deposits,
            locked_funds,
            vesting_funds,
            fee_debt,
            initial_pledge,
            pre_committed_sectors,
            pre_committed_sectors_cleanup,
            allocated_sectors,
            sectors,
            proving_period_start,
            current_deadline,
            deadlines,
            early_terminations,
            deadline_cron_active,
        })
    }

    pub fn info<BS: Blockstore>(&self, store: &BS) -> anyhow::Result<MinerInfo> {
        delegate_state!(self => |st| Ok(st.get_info(store)?.into()))
    }

    /// Loads deadlines for a miner's state
    pub fn for_each_deadline<BS: Blockstore>(
        &self,
        policy: &Policy,
        store: &BS,
        mut f: impl FnMut(u64, Deadline) -> Result<(), anyhow::Error>,
    ) -> anyhow::Result<()> {
        match self {
            State::V8(st) => {
                st.load_deadlines(&store)?
                    .for_each(&policy.into(), &store, |idx, dl| f(idx, dl.into()))
            }
            State::V9(st) => {
                st.load_deadlines(&store)?
                    .for_each(&policy.into(), &store, |idx, dl| f(idx, dl.into()))
            }
            State::V10(st) => {
                st.load_deadlines(&store)?
                    .for_each(&policy.into(), &store, |idx, dl| f(idx, dl.into()))
            }
            State::V11(st) => {
                st.load_deadlines(&store)?
                    .for_each(&policy.into(), &store, |idx, dl| f(idx, dl.into()))
            }
            State::V12(st) => st
                .load_deadlines(store)?
                .for_each(store, |idx, dl| f(idx, dl.into())),
            State::V13(st) => st
                .load_deadlines(store)?
                .for_each(store, |idx, dl| f(idx, dl.into())),
            State::V14(st) => st
                .load_deadlines(store)?
                .for_each(store, |idx, dl| f(idx, dl.into())),
            State::V15(st) => st
                .load_deadlines(store)?
                .for_each(store, |idx, dl| f(idx, dl.into())),
            State::V16(st) => st
                .load_deadlines(store)?
                .for_each(store, |idx, dl| f(idx, dl.into())),
            State::V17(st) => st
                .load_deadlines(store)?
                .for_each(store, |idx, dl| f(idx, dl.into())),
        }
    }

    /// Loads deadline at index for a miner's state
    pub fn load_deadline<BS: Blockstore>(
        &self,
        policy: &Policy,
        store: &BS,
        idx: u64,
    ) -> anyhow::Result<Deadline> {
        match self {
            State::V8(st) => Ok(st
                .load_deadlines(store)?
                .load_deadline(&policy.into(), store, idx)
                .map(From::from)?),
            State::V9(st) => Ok(st
                .load_deadlines(store)?
                .load_deadline(&policy.into(), store, idx)
                .map(From::from)?),
            State::V10(st) => Ok(st
                .load_deadlines(store)?
                .load_deadline(&policy.into(), store, idx)
                .map(From::from)?),
            State::V11(st) => Ok(st
                .load_deadlines(store)?
                .load_deadline(&policy.into(), store, idx)
                .map(From::from)?),
            State::V12(st) => Ok(st
                .load_deadlines(store)?
                .load_deadline(store, idx)
                .map(From::from)?),
            State::V13(st) => Ok(st
                .load_deadlines(store)?
                .load_deadline(store, idx)
                .map(From::from)?),
            State::V14(st) => Ok(st
                .load_deadlines(store)?
                .load_deadline(store, idx)
                .map(From::from)?),
            State::V15(st) => Ok(st
                .load_deadlines(store)?
                .load_deadline(store, idx)
                .map(From::from)?),
            State::V16(st) => Ok(st
                .load_deadlines(store)?
                .load_deadline(store, idx)
                .map(From::from)?),
            State::V17(st) => Ok(st
                .load_deadlines(store)?
                .load_deadline(store, idx)
                .map(From::from)?),
        }
    }

    /// Loads sectors corresponding to the bitfield. If no bitfield is passed
    /// in, return all.
    pub fn load_sectors<BS: Blockstore>(
        &self,
        store: &BS,
        sectors: Option<&BitField>,
    ) -> anyhow::Result<Vec<SectorOnChainInfo>> {
        match self {
            State::V8(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_state::v8::Sectors::load(&store, &st.sectors)?;
                    let mut infos = Vec::with_capacity(sectors.amt.count() as usize);
                    sectors.amt.for_each(|_, info| {
                        infos.push(SectorOnChainInfo::from(info.clone()));
                        Ok(())
                    })?;
                    Ok(infos)
                }
            }
            State::V9(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_state::v9::Sectors::load(&store, &st.sectors)?;
                    let mut infos = Vec::with_capacity(sectors.amt.count() as usize);
                    sectors.amt.for_each(|_, info| {
                        infos.push(SectorOnChainInfo::from(info.clone()));
                        Ok(())
                    })?;
                    Ok(infos)
                }
            }
            State::V10(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_state::v10::Sectors::load(&store, &st.sectors)?;
                    let mut infos = Vec::with_capacity(sectors.amt.count() as usize);
                    sectors.amt.for_each(|_, info| {
                        infos.push(SectorOnChainInfo::from(info.clone()));
                        Ok(())
                    })?;
                    Ok(infos)
                }
            }
            State::V11(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_state::v11::Sectors::load(&store, &st.sectors)?;
                    let mut infos = Vec::with_capacity(sectors.amt.count() as usize);
                    sectors.amt.for_each(|_, info| {
                        infos.push(SectorOnChainInfo::from(info.clone()));
                        Ok(())
                    })?;
                    Ok(infos)
                }
            }
            State::V12(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_state::v12::Sectors::load(&store, &st.sectors)?;
                    let mut infos = Vec::with_capacity(sectors.amt.count() as usize);
                    sectors.amt.for_each(|_, info| {
                        infos.push(SectorOnChainInfo::from(info.clone()));
                        Ok(())
                    })?;
                    Ok(infos)
                }
            }
            State::V13(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_state::v13::Sectors::load(&store, &st.sectors)?;
                    let mut infos = Vec::with_capacity(sectors.amt.count() as usize);
                    sectors.amt.for_each(|_, info| {
                        infos.push(SectorOnChainInfo::from(info.clone()));
                        Ok(())
                    })?;
                    Ok(infos)
                }
            }
            State::V14(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_state::v14::Sectors::load(&store, &st.sectors)?;
                    let mut infos = Vec::with_capacity(sectors.amt.count() as usize);
                    sectors.amt.for_each(|_, info| {
                        infos.push(SectorOnChainInfo::from(info.clone()));
                        Ok(())
                    })?;
                    Ok(infos)
                }
            }
            State::V15(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_state::v15::Sectors::load(&store, &st.sectors)?;
                    let mut infos = Vec::with_capacity(sectors.amt.count() as usize);
                    sectors.amt.for_each(|_, info| {
                        infos.push(SectorOnChainInfo::from(info.clone()));
                        Ok(())
                    })?;
                    Ok(infos)
                }
            }
            State::V16(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_state::v16::Sectors::load(&store, &st.sectors)?;
                    let mut infos = Vec::with_capacity(sectors.amt.count() as usize);
                    sectors.amt.for_each(|_, info| {
                        infos.push(SectorOnChainInfo::from(info.clone()));
                        Ok(())
                    })?;
                    Ok(infos)
                }
            }
            State::V17(st) => {
                if let Some(sectors) = sectors {
                    Ok(st
                        .load_sector_infos(&store, sectors)?
                        .into_iter()
                        .map(From::from)
                        .collect())
                } else {
                    let sectors = fil_actor_miner_state::v17::Sectors::load(&store, &st.sectors)?;
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

    /// Returns the deadline and partition index for a sector number.
    pub fn find_sector<BS: Blockstore>(
        &self,
        store: &BS,
        sector_number: SectorNumber,
        policy: &Policy,
    ) -> anyhow::Result<(u64, u64)> {
        match self {
            State::V8(st) => st.find_sector(&policy.into(), store, sector_number),
            State::V9(st) => st.find_sector(&policy.into(), store, sector_number),
            State::V10(st) => st.find_sector(&policy.into(), store, sector_number),
            State::V11(st) => st.find_sector(&policy.into(), store, sector_number),
            State::V12(st) => st.find_sector(store, sector_number),
            State::V13(st) => st.find_sector(store, sector_number),
            State::V14(st) => st.find_sector(store, sector_number),
            State::V15(st) => st.find_sector(store, sector_number),
            State::V16(st) => st.find_sector(store, sector_number),
            State::V17(st) => st.find_sector(store, sector_number),
        }
    }

    /// Gets fee debt of miner state
    pub fn fee_debt(&self) -> TokenAmount {
        delegate_state!(self.fee_debt.clone().into())
    }

    /// Unclaimed funds. Actor balance - (locked funds, precommit deposit, ip requirement) Can go negative if the miner is in IP debt.
    pub fn available_balance(&self, balance: &BigInt) -> anyhow::Result<TokenAmount> {
        let balance: TokenAmount = TokenAmount::from_atto(balance.clone());
        Ok(delegate_state!(
            self.get_available_balance(&balance.into())?.into()
        ))
    }

    /// Returns deadline calculations for the current (according to state) proving period.
    pub fn deadline_info(&self, policy: &Policy, current_epoch: ChainEpoch) -> DeadlineInfo {
        delegate_state!(self.deadline_info(&policy.into(), current_epoch).into())
    }

    pub fn allocated_sectors(&self) -> Cid {
        delegate_state!(self.allocated_sectors)
    }

    /// Loads the allocated sector numbers
    pub fn load_allocated_sector_numbers<BS: Blockstore>(
        &self,
        store: &BS,
    ) -> anyhow::Result<BitField> {
        store.get_cbor_required(&self.allocated_sectors())
    }

    /// Loads the precommit-on-chain info
    pub fn load_precommit_on_chain_info<BS: Blockstore>(
        &self,
        store: &BS,
        sector_number: u64,
    ) -> anyhow::Result<Option<SectorPreCommitOnChainInfo>> {
        Ok(delegate_state!(
            self.get_precommitted_sector(store, sector_number)?
                .map(From::from)
        ))
    }

    /// Returns deadline calculations for the state recorded proving period and deadline.
    /// This is out of date if the a miner does not have an active miner cron
    pub fn recorded_deadline_info(
        &self,
        policy: &Policy,
        current_epoch: ChainEpoch,
    ) -> DeadlineInfo {
        delegate_state!(
            self.recorded_deadline_info(&policy.into(), current_epoch)
                .into()
        )
    }
}

/// Static information about miner
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MinerInfo {
    pub owner: Address,
    pub worker: Address,
    pub new_worker: Option<Address>,
    pub control_addresses: Vec<Address>, // Must all be ID addresses.
    pub worker_change_epoch: ChainEpoch,
    #[serde(with = "serde_bytes")]
    pub peer_id: Vec<u8>,
    pub multiaddrs: Vec<BytesDe>,
    pub window_post_proof_type: RegisteredPoStProof,
    pub sector_size: SectorSize,
    pub window_post_partition_sectors: u64,
    pub consensus_fault_elapsed: ChainEpoch,
    pub pending_owner_address: Option<Address>,
    pub beneficiary: Address,
    pub beneficiary_term: BeneficiaryTerm,
    pub pending_beneficiary_term: Option<PendingBeneficiaryChange>,
}

impl From<fil_actor_miner_state::v8::MinerInfo> for MinerInfo {
    fn from(info: fil_actor_miner_state::v8::MinerInfo) -> Self {
        MinerInfo {
            owner: info.owner.into(),
            worker: info.worker.into(),
            control_addresses: info.control_addresses.into_iter().map(From::from).collect(),
            new_worker: info
                .pending_worker_key
                .as_ref()
                .map(|k| k.new_worker.into()),
            worker_change_epoch: info
                .pending_worker_key
                .map(|k| k.effective_at)
                .unwrap_or(-1),
            peer_id: info.peer_id,
            multiaddrs: info.multi_address,
            window_post_proof_type: info.window_post_proof_type,
            sector_size: info.sector_size.into(),
            window_post_partition_sectors: info.window_post_partition_sectors,
            consensus_fault_elapsed: info.consensus_fault_elapsed,
            pending_owner_address: info.pending_owner_address.map(From::from),
            beneficiary: info.owner.into(),
            beneficiary_term: BeneficiaryTerm::default(),
            pending_beneficiary_term: None,
        }
    }
}

impl From<fil_actor_miner_state::v9::MinerInfo> for MinerInfo {
    fn from(info: fil_actor_miner_state::v9::MinerInfo) -> Self {
        MinerInfo {
            owner: info.owner.into(),
            worker: info.worker.into(),
            control_addresses: info.control_addresses.into_iter().map(From::from).collect(),
            new_worker: info
                .pending_worker_key
                .as_ref()
                .map(|k| k.new_worker.into()),
            worker_change_epoch: info
                .pending_worker_key
                .map(|k| k.effective_at)
                .unwrap_or(-1),
            peer_id: info.peer_id,
            multiaddrs: info.multi_address,
            window_post_proof_type: info.window_post_proof_type,
            sector_size: info.sector_size.into(),
            window_post_partition_sectors: info.window_post_partition_sectors,
            consensus_fault_elapsed: info.consensus_fault_elapsed,
            pending_owner_address: info.pending_owner_address.map(From::from),
            beneficiary: info.beneficiary.into(),
            beneficiary_term: BeneficiaryTerm {
                expiration: info.beneficiary_term.expiration,
                quota: from_token_v2_to_v4(&info.beneficiary_term.quota),
                used_quota: from_token_v2_to_v4(&info.beneficiary_term.used_quota),
            },
            pending_beneficiary_term: info.pending_beneficiary_term.map(|term| {
                PendingBeneficiaryChange {
                    new_beneficiary: from_address_v2_to_v4(term.new_beneficiary),
                    new_quota: from_token_v2_to_v4(&term.new_quota),
                    new_expiration: term.new_expiration,
                    approved_by_beneficiary: term.approved_by_beneficiary,
                    approved_by_nominee: term.approved_by_nominee,
                }
            }),
        }
    }
}

impl From<fil_actor_miner_state::v10::MinerInfo> for MinerInfo {
    fn from(info: fil_actor_miner_state::v10::MinerInfo) -> Self {
        MinerInfo {
            owner: info.owner.into(),
            worker: info.worker.into(),
            control_addresses: info.control_addresses.into_iter().map(From::from).collect(),
            new_worker: info
                .pending_worker_key
                .as_ref()
                .map(|k| k.new_worker.into()),
            worker_change_epoch: info
                .pending_worker_key
                .map(|k| k.effective_at)
                .unwrap_or(-1),
            peer_id: info.peer_id,
            multiaddrs: info.multi_address,
            window_post_proof_type: from_reg_post_proof_v3_to_v2(info.window_post_proof_type),
            sector_size: info.sector_size.into(),
            window_post_partition_sectors: info.window_post_partition_sectors,
            consensus_fault_elapsed: info.consensus_fault_elapsed,
            pending_owner_address: info.pending_owner_address.map(From::from),
            beneficiary: info.beneficiary.into(),
            beneficiary_term: BeneficiaryTerm {
                quota: from_token_v3_to_v4(&info.beneficiary_term.quota),
                used_quota: from_token_v3_to_v4(&info.beneficiary_term.used_quota),
                expiration: info.beneficiary_term.expiration,
            },
            pending_beneficiary_term: info.pending_beneficiary_term.map(|term| {
                PendingBeneficiaryChange {
                    new_beneficiary: from_address_v3_to_v4(term.new_beneficiary),
                    new_quota: from_token_v3_to_v4(&term.new_quota),
                    new_expiration: term.new_expiration,
                    approved_by_beneficiary: term.approved_by_beneficiary,
                    approved_by_nominee: term.approved_by_nominee,
                }
            }),
        }
    }
}

impl From<fil_actor_miner_state::v11::MinerInfo> for MinerInfo {
    fn from(info: fil_actor_miner_state::v11::MinerInfo) -> Self {
        MinerInfo {
            owner: info.owner.into(),
            worker: info.worker.into(),
            control_addresses: info.control_addresses.into_iter().map(From::from).collect(),
            new_worker: info
                .pending_worker_key
                .as_ref()
                .map(|k| k.new_worker.into()),
            worker_change_epoch: info
                .pending_worker_key
                .map(|k| k.effective_at)
                .unwrap_or(-1),
            peer_id: info.peer_id,
            multiaddrs: info.multi_address,
            window_post_proof_type: from_reg_post_proof_v3_to_v2(info.window_post_proof_type),
            sector_size: info.sector_size.into(),
            window_post_partition_sectors: info.window_post_partition_sectors,
            consensus_fault_elapsed: info.consensus_fault_elapsed,
            pending_owner_address: info.pending_owner_address.map(From::from),
            beneficiary: info.beneficiary.into(),
            beneficiary_term: BeneficiaryTerm {
                quota: from_token_v3_to_v4(&info.beneficiary_term.quota),
                used_quota: from_token_v3_to_v4(&info.beneficiary_term.used_quota),
                expiration: info.beneficiary_term.expiration,
            },
            pending_beneficiary_term: info.pending_beneficiary_term.map(|change| {
                PendingBeneficiaryChange {
                    new_beneficiary: from_address_v3_to_v4(change.new_beneficiary),
                    new_quota: from_token_v3_to_v4(&change.new_quota),
                    new_expiration: change.new_expiration,
                    approved_by_beneficiary: change.approved_by_beneficiary,
                    approved_by_nominee: change.approved_by_nominee,
                }
            }),
        }
    }
}

impl From<fil_actor_miner_state::v12::MinerInfo> for MinerInfo {
    fn from(info: fil_actor_miner_state::v12::MinerInfo) -> Self {
        MinerInfo {
            owner: info.owner.into(),
            worker: info.worker.into(),
            control_addresses: info.control_addresses.into_iter().map(From::from).collect(),
            new_worker: info
                .pending_worker_key
                .as_ref()
                .map(|k| k.new_worker.into()),
            worker_change_epoch: info
                .pending_worker_key
                .map(|k| k.effective_at)
                .unwrap_or(-1),
            peer_id: info.peer_id,
            multiaddrs: info.multi_address,
            window_post_proof_type: from_reg_post_proof_v4_to_v2(info.window_post_proof_type),
            sector_size: info.sector_size.into(),
            window_post_partition_sectors: info.window_post_partition_sectors,
            consensus_fault_elapsed: info.consensus_fault_elapsed,
            pending_owner_address: info.pending_owner_address.map(From::from),
            beneficiary: info.beneficiary.into(),
            beneficiary_term: info.beneficiary_term,
            pending_beneficiary_term: info.pending_beneficiary_term,
        }
    }
}

impl From<fil_actor_miner_state::v13::MinerInfo> for MinerInfo {
    fn from(info: fil_actor_miner_state::v13::MinerInfo) -> Self {
        MinerInfo {
            owner: info.owner.into(),
            worker: info.worker.into(),
            control_addresses: info.control_addresses.into_iter().map(From::from).collect(),
            new_worker: info
                .pending_worker_key
                .as_ref()
                .map(|k| k.new_worker.into()),
            worker_change_epoch: info
                .pending_worker_key
                .map(|k| k.effective_at)
                .unwrap_or(-1),
            peer_id: info.peer_id,
            multiaddrs: info.multi_address,
            window_post_proof_type: from_reg_post_proof_v4_to_v2(info.window_post_proof_type),
            sector_size: info.sector_size.into(),
            window_post_partition_sectors: info.window_post_partition_sectors,
            consensus_fault_elapsed: info.consensus_fault_elapsed,
            pending_owner_address: info.pending_owner_address.map(From::from),
            beneficiary: info.beneficiary.into(),
            beneficiary_term: BeneficiaryTerm {
                quota: info.beneficiary_term.quota,
                used_quota: info.beneficiary_term.used_quota,
                expiration: info.beneficiary_term.expiration,
            },
            pending_beneficiary_term: info.pending_beneficiary_term.map(|change| {
                PendingBeneficiaryChange {
                    new_beneficiary: change.new_beneficiary,
                    new_quota: change.new_quota,
                    new_expiration: change.new_expiration,
                    approved_by_beneficiary: change.approved_by_beneficiary,
                    approved_by_nominee: change.approved_by_nominee,
                }
            }),
        }
    }
}

impl From<fil_actor_miner_state::v14::MinerInfo> for MinerInfo {
    fn from(info: fil_actor_miner_state::v14::MinerInfo) -> Self {
        MinerInfo {
            owner: info.owner.into(),
            worker: info.worker.into(),
            control_addresses: info.control_addresses.into_iter().map(From::from).collect(),
            new_worker: info
                .pending_worker_key
                .as_ref()
                .map(|k| k.new_worker.into()),
            worker_change_epoch: info
                .pending_worker_key
                .map(|k| k.effective_at)
                .unwrap_or(-1),
            peer_id: info.peer_id,
            multiaddrs: info.multi_address,
            window_post_proof_type: from_reg_post_proof_v4_to_v2(info.window_post_proof_type),
            sector_size: info.sector_size.into(),
            window_post_partition_sectors: info.window_post_partition_sectors,
            consensus_fault_elapsed: info.consensus_fault_elapsed,
            pending_owner_address: info.pending_owner_address.map(From::from),
            beneficiary: info.beneficiary.into(),
            beneficiary_term: BeneficiaryTerm {
                quota: info.beneficiary_term.quota,
                used_quota: info.beneficiary_term.used_quota,
                expiration: info.beneficiary_term.expiration,
            },
            pending_beneficiary_term: info.pending_beneficiary_term.map(|change| {
                PendingBeneficiaryChange {
                    new_beneficiary: change.new_beneficiary,
                    new_quota: change.new_quota,
                    new_expiration: change.new_expiration,
                    approved_by_beneficiary: change.approved_by_beneficiary,
                    approved_by_nominee: change.approved_by_nominee,
                }
            }),
        }
    }
}

impl From<fil_actor_miner_state::v15::MinerInfo> for MinerInfo {
    fn from(info: fil_actor_miner_state::v15::MinerInfo) -> Self {
        MinerInfo {
            owner: info.owner.into(),
            worker: info.worker.into(),
            control_addresses: info.control_addresses.into_iter().map(From::from).collect(),
            new_worker: info
                .pending_worker_key
                .as_ref()
                .map(|k| k.new_worker.into()),
            worker_change_epoch: info
                .pending_worker_key
                .map(|k| k.effective_at)
                .unwrap_or(-1),
            peer_id: info.peer_id,
            multiaddrs: info.multi_address,
            window_post_proof_type: from_reg_post_proof_v4_to_v2(info.window_post_proof_type),
            sector_size: info.sector_size.into(),
            window_post_partition_sectors: info.window_post_partition_sectors,
            consensus_fault_elapsed: info.consensus_fault_elapsed,
            pending_owner_address: info.pending_owner_address.map(From::from),
            beneficiary: info.beneficiary.into(),
            beneficiary_term: BeneficiaryTerm {
                quota: info.beneficiary_term.quota,
                used_quota: info.beneficiary_term.used_quota,
                expiration: info.beneficiary_term.expiration,
            },
            pending_beneficiary_term: info.pending_beneficiary_term.map(|change| {
                PendingBeneficiaryChange {
                    new_beneficiary: change.new_beneficiary,
                    new_quota: change.new_quota,
                    new_expiration: change.new_expiration,
                    approved_by_beneficiary: change.approved_by_beneficiary,
                    approved_by_nominee: change.approved_by_nominee,
                }
            }),
        }
    }
}

impl From<fil_actor_miner_state::v16::MinerInfo> for MinerInfo {
    fn from(info: fil_actor_miner_state::v16::MinerInfo) -> Self {
        MinerInfo {
            owner: info.owner.into(),
            worker: info.worker.into(),
            control_addresses: info.control_addresses.into_iter().map(From::from).collect(),
            new_worker: info
                .pending_worker_key
                .as_ref()
                .map(|k| k.new_worker.into()),
            worker_change_epoch: info
                .pending_worker_key
                .map(|k| k.effective_at)
                .unwrap_or(-1),
            peer_id: info.peer_id,
            multiaddrs: info.multi_address,
            window_post_proof_type: from_reg_post_proof_v4_to_v2(info.window_post_proof_type),
            sector_size: info.sector_size.into(),
            window_post_partition_sectors: info.window_post_partition_sectors,
            consensus_fault_elapsed: info.consensus_fault_elapsed,
            pending_owner_address: info.pending_owner_address.map(From::from),
            beneficiary: info.beneficiary.into(),
            beneficiary_term: BeneficiaryTerm {
                quota: info.beneficiary_term.quota,
                used_quota: info.beneficiary_term.used_quota,
                expiration: info.beneficiary_term.expiration,
            },
            pending_beneficiary_term: info.pending_beneficiary_term.map(|change| {
                PendingBeneficiaryChange {
                    new_beneficiary: change.new_beneficiary,
                    new_quota: change.new_quota,
                    new_expiration: change.new_expiration,
                    approved_by_beneficiary: change.approved_by_beneficiary,
                    approved_by_nominee: change.approved_by_nominee,
                }
            }),
        }
    }
}

impl From<fil_actor_miner_state::v17::MinerInfo> for MinerInfo {
    fn from(info: fil_actor_miner_state::v17::MinerInfo) -> Self {
        MinerInfo {
            owner: info.owner.into(),
            worker: info.worker.into(),
            control_addresses: info.control_addresses.into_iter().map(From::from).collect(),
            new_worker: info
                .pending_worker_key
                .as_ref()
                .map(|k| k.new_worker.into()),
            worker_change_epoch: info
                .pending_worker_key
                .map(|k| k.effective_at)
                .unwrap_or(-1),
            peer_id: info.peer_id,
            multiaddrs: info.multi_address,
            window_post_proof_type: from_reg_post_proof_v4_to_v2(info.window_post_proof_type),
            sector_size: info.sector_size.into(),
            window_post_partition_sectors: info.window_post_partition_sectors,
            consensus_fault_elapsed: info.consensus_fault_elapsed,
            pending_owner_address: info.pending_owner_address.map(From::from),
            beneficiary: info.beneficiary.into(),
            beneficiary_term: BeneficiaryTerm {
                quota: info.beneficiary_term.quota,
                used_quota: info.beneficiary_term.used_quota,
                expiration: info.beneficiary_term.expiration,
            },
            pending_beneficiary_term: info.pending_beneficiary_term.map(|change| {
                PendingBeneficiaryChange {
                    new_beneficiary: change.new_beneficiary,
                    new_quota: change.new_quota,
                    new_expiration: change.new_expiration,
                    approved_by_beneficiary: change.approved_by_beneficiary,
                    approved_by_nominee: change.approved_by_nominee,
                }
            }),
        }
    }
}

impl MinerInfo {
    pub fn worker(&self) -> Address {
        self.worker
    }

    pub fn sector_size(&self) -> SectorSize {
        self.sector_size
    }
}

#[derive(Clone, Serialize, Deserialize, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct MinerPower {
    pub miner_power: Claim,
    pub total_power: Claim,
    pub has_min_power: bool,
}

/// Deadline holds the state for all sectors due at a specific deadline.
#[delegated_enum(impl_conversions)]
pub enum Deadline {
    V8(fil_actor_miner_state::v8::Deadline),
    V9(fil_actor_miner_state::v9::Deadline),
    V10(fil_actor_miner_state::v10::Deadline),
    V11(fil_actor_miner_state::v11::Deadline),
    V12(fil_actor_miner_state::v12::Deadline),
    V13(fil_actor_miner_state::v13::Deadline),
    V14(fil_actor_miner_state::v14::Deadline),
    V15(fil_actor_miner_state::v15::Deadline),
    V16(fil_actor_miner_state::v16::Deadline),
    V17(fil_actor_miner_state::v17::Deadline),
}

impl Deadline {
    /// For each partition of the deadline
    pub fn for_each<BS: Blockstore>(
        &self,
        store: &BS,
        mut f: impl FnMut(u64, Partition) -> Result<(), anyhow::Error>,
    ) -> anyhow::Result<()> {
        delegate_deadline!(self.for_each(&store, |idx, part| f(idx, Cow::Borrowed(part).into())))
    }

    /// Returns number of partitions posted
    pub fn partitions_posted(&self) -> BitField {
        delegate_deadline!(self.partitions_posted.clone())
    }

    /// Returns disputable proof count of the deadline
    pub fn disputable_proof_count<BS: Blockstore>(&self, store: &BS) -> anyhow::Result<u64> {
        Ok(delegate_deadline!(
            self.optimistic_proofs_snapshot_amt(store)?.count()
        ))
    }
}

#[delegated_enum(impl_conversions)]
pub enum Partition<'a> {
    V8(Cow<'a, fil_actor_miner_state::v8::Partition>),
    V9(Cow<'a, fil_actor_miner_state::v9::Partition>),
    V10(Cow<'a, fil_actor_miner_state::v10::Partition>),
    V11(Cow<'a, fil_actor_miner_state::v11::Partition>),
    V12(Cow<'a, fil_actor_miner_state::v12::Partition>),
    V13(Cow<'a, fil_actor_miner_state::v13::Partition>),
    V14(Cow<'a, fil_actor_miner_state::v14::Partition>),
    V15(Cow<'a, fil_actor_miner_state::v15::Partition>),
    V16(Cow<'a, fil_actor_miner_state::v16::Partition>),
    V17(Cow<'a, fil_actor_miner_state::v17::Partition>),
}

impl Partition<'_> {
    pub fn all_sectors(&self) -> &BitField {
        delegate_partition!(self.sectors.borrow())
    }
    pub fn faulty_sectors(&self) -> &BitField {
        delegate_partition!(self.faults.borrow())
    }
    pub fn live_sectors(&self) -> BitField {
        delegate_partition!(self.live_sectors())
    }
    pub fn active_sectors(&self) -> BitField {
        delegate_partition!(self.active_sectors())
    }
    pub fn recovering_sectors(&self) -> &BitField {
        delegate_partition!(self.recoveries.borrow())
    }

    /// Terminated sectors
    pub fn terminated(&self) -> &BitField {
        delegate_partition!(self.terminated.borrow())
    }

    // Maps epochs sectors that expire in or before that epoch.
    // An expiration may be an "on-time" scheduled expiration, or early "faulty" expiration.
    // Keys are quantized to last-in-deadline epochs.
    pub fn expirations_epochs(&self) -> Cid {
        delegate_partition!(self.expirations_epochs)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SectorOnChainInfo {
    pub sector_number: SectorNumber,
    /// The seal proof type implies the PoSt proofs
    pub seal_proof: RegisteredSealProof,
    /// `CommR`
    pub sealed_cid: Cid,
    pub deal_ids: Vec<DealID>,
    /// Epoch during which the sector proof was accepted
    pub activation: ChainEpoch,
    /// Epoch during which the sector expires
    pub expiration: ChainEpoch,
    /// Integral of active deals over sector lifetime
    pub deal_weight: BigInt,
    /// Integral of active verified deals over sector lifetime
    pub verified_deal_weight: BigInt,
    /// Pledge collected to commit this sector
    pub initial_pledge: TokenAmount,
    /// Expected one day projection of reward for sector computed at activation
    /// time
    pub expected_day_reward: TokenAmount,
    /// Expected twenty day projection of reward for sector computed at
    /// activation time
    pub expected_storage_pledge: TokenAmount,
    /// Age of sector this sector replaced or zero
    pub replaced_sector_age: ChainEpoch,
    /// Day reward of sector this sector replace or zero
    pub replaced_day_reward: TokenAmount,
    /// The original `SealedSectorCID`, only gets set on the first `ReplicaUpdate`
    pub sector_key_cid: Option<Cid>,
    // Flag for QA power mechanism introduced in fip 0045
    pub simple_qa_power: bool,
}

impl From<fil_actor_miner_state::v8::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: fil_actor_miner_state::v8::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: info.seal_proof,
            sealed_cid: info.sealed_cid,
            deal_ids: info.deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge.into(),
            expected_day_reward: info.expected_day_reward.into(),
            expected_storage_pledge: info.expected_storage_pledge.into(),
            replaced_sector_age: ChainEpoch::default(),
            replaced_day_reward: TokenAmount::default(),
            sector_key_cid: info.sector_key_cid,
            simple_qa_power: bool::default(),
        }
    }
}

impl From<fil_actor_miner_state::v9::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: fil_actor_miner_state::v9::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: info.seal_proof,
            sealed_cid: info.sealed_cid,
            deal_ids: info.deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge.into(),
            expected_day_reward: info.expected_day_reward.into(),
            expected_storage_pledge: info.expected_storage_pledge.into(),
            replaced_sector_age: info.replaced_sector_age,
            replaced_day_reward: info.replaced_day_reward.into(),
            sector_key_cid: info.sector_key_cid,
            simple_qa_power: info.simple_qa_power,
        }
    }
}

impl From<fil_actor_miner_state::v10::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: fil_actor_miner_state::v10::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: from_reg_seal_proof_v3_to_v2(info.seal_proof),
            sealed_cid: info.sealed_cid,
            deal_ids: info.deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge.into(),
            expected_day_reward: info.expected_day_reward.into(),
            expected_storage_pledge: info.expected_storage_pledge.into(),
            replaced_sector_age: info.replaced_sector_age,
            replaced_day_reward: info.replaced_day_reward.into(),
            sector_key_cid: info.sector_key_cid,
            simple_qa_power: info.simple_qa_power,
        }
    }
}

impl From<fil_actor_miner_state::v11::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: fil_actor_miner_state::v11::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: from_reg_seal_proof_v3_to_v2(info.seal_proof),
            sealed_cid: info.sealed_cid,
            deal_ids: info.deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge.into(),
            expected_day_reward: info.expected_day_reward.into(),
            expected_storage_pledge: info.expected_storage_pledge.into(),
            replaced_sector_age: info.replaced_sector_age,
            replaced_day_reward: info.replaced_day_reward.into(),
            sector_key_cid: info.sector_key_cid,
            simple_qa_power: info.simple_qa_power,
        }
    }
}

impl From<fil_actor_miner_state::v12::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: fil_actor_miner_state::v12::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: from_reg_seal_proof_v4_to_v2(info.seal_proof),
            sealed_cid: info.sealed_cid,
            deal_ids: info.deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge.into(),
            expected_day_reward: info.expected_day_reward.into(),
            expected_storage_pledge: info.expected_storage_pledge.into(),
            replaced_sector_age: ChainEpoch::default(),
            replaced_day_reward: info.replaced_day_reward.into(),
            sector_key_cid: info.sector_key_cid,
            simple_qa_power: bool::default(),
        }
    }
}

impl From<fil_actor_miner_state::v13::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: fil_actor_miner_state::v13::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: from_reg_seal_proof_v4_to_v2(info.seal_proof),
            sealed_cid: info.sealed_cid,
            deal_ids: info.deprecated_deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge.into(),
            expected_day_reward: info.expected_day_reward.into(),
            expected_storage_pledge: info.expected_storage_pledge.into(),
            replaced_sector_age: ChainEpoch::default(),
            replaced_day_reward: info.replaced_day_reward.into(),
            sector_key_cid: info.sector_key_cid,
            simple_qa_power: bool::default(),
        }
    }
}

impl From<fil_actor_miner_state::v14::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: fil_actor_miner_state::v14::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: from_reg_seal_proof_v4_to_v2(info.seal_proof),
            sealed_cid: info.sealed_cid,
            deal_ids: info.deprecated_deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge.into(),
            expected_day_reward: info.expected_day_reward.into(),
            expected_storage_pledge: info.expected_storage_pledge.into(),
            replaced_sector_age: ChainEpoch::default(),
            replaced_day_reward: info.replaced_day_reward.into(),
            sector_key_cid: info.sector_key_cid,
            simple_qa_power: bool::default(),
        }
    }
}

impl From<fil_actor_miner_state::v15::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: fil_actor_miner_state::v15::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: from_reg_seal_proof_v4_to_v2(info.seal_proof),
            sealed_cid: info.sealed_cid,
            deal_ids: info.deprecated_deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge.into(),
            expected_day_reward: info.expected_day_reward.into(),
            expected_storage_pledge: info.expected_storage_pledge.into(),
            replaced_sector_age: ChainEpoch::default(),
            replaced_day_reward: info.replaced_day_reward.into(),
            sector_key_cid: info.sector_key_cid,
            simple_qa_power: bool::default(),
        }
    }
}

impl From<fil_actor_miner_state::v16::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: fil_actor_miner_state::v16::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: from_reg_seal_proof_v4_to_v2(info.seal_proof),
            sealed_cid: info.sealed_cid,
            deal_ids: info.deprecated_deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge.into(),
            expected_day_reward: info.expected_day_reward.map(From::from).unwrap_or_default(),
            expected_storage_pledge: info
                .expected_storage_pledge
                .map(From::from)
                .unwrap_or_default(),
            replaced_sector_age: ChainEpoch::default(),
            replaced_day_reward: info.replaced_day_reward.map(From::from).unwrap_or_default(),
            sector_key_cid: info.sector_key_cid,
            simple_qa_power: bool::default(),
        }
    }
}

impl From<fil_actor_miner_state::v17::SectorOnChainInfo> for SectorOnChainInfo {
    fn from(info: fil_actor_miner_state::v17::SectorOnChainInfo) -> Self {
        Self {
            sector_number: info.sector_number,
            seal_proof: from_reg_seal_proof_v4_to_v2(info.seal_proof),
            sealed_cid: info.sealed_cid,
            deal_ids: info.deprecated_deal_ids,
            activation: info.activation,
            expiration: info.expiration,
            deal_weight: info.deal_weight,
            verified_deal_weight: info.verified_deal_weight,
            initial_pledge: info.initial_pledge.into(),
            expected_day_reward: info.expected_day_reward.map(From::from).unwrap_or_default(),
            expected_storage_pledge: info
                .expected_storage_pledge
                .map(From::from)
                .unwrap_or_default(),
            replaced_sector_age: ChainEpoch::default(),
            replaced_day_reward: info.replaced_day_reward.map(From::from).unwrap_or_default(),
            sector_key_cid: info.sector_key_cid,
            simple_qa_power: bool::default(),
        }
    }
}

/// Deadline calculations with respect to a current epoch.
/// "Deadline" refers to the window during which proofs may be submitted.
/// Windows are non-overlapping ranges [Open, Close), but the challenge epoch for a window occurs
/// before the window opens.
#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Eq, Copy, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct DeadlineInfo {
    /// Epoch at which this info was calculated.
    pub current_epoch: ChainEpoch,
    /// First epoch of the proving period (<= CurrentEpoch).
    pub period_start: ChainEpoch,
    /// Current deadline index, in [0..WPoStProvingPeriodDeadlines).
    pub index: u64,
    /// First epoch from which a proof may be submitted (>= CurrentEpoch).
    pub open: ChainEpoch,
    /// First epoch from which a proof may no longer be submitted (>= Open).
    pub close: ChainEpoch,
    /// Epoch at which to sample the chain for challenge (< Open).
    pub challenge: ChainEpoch,
    /// First epoch at which a fault declaration is rejected (< Open).
    pub fault_cutoff: ChainEpoch,

    // Protocol parameters (This is intentionally included in the JSON response for deadlines)
    #[serde(rename = "WPoStPeriodDeadlines")]
    w_post_period_deadlines: u64,
    #[serde(rename = "WPoStProvingPeriod")]
    w_post_proving_period: ChainEpoch,
    #[serde(rename = "WPoStChallengeWindow")]
    w_post_challenge_window: ChainEpoch,
    #[serde(rename = "WPoStChallengeLookback")]
    w_post_challenge_lookback: ChainEpoch,
    fault_declaration_cutoff: ChainEpoch,
}

impl DeadlineInfo {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        period_start: ChainEpoch,
        deadline_idx: u64,
        current_epoch: ChainEpoch,
        w_post_period_deadlines: u64,
        w_post_proving_period: ChainEpoch,
        w_post_challenge_window: ChainEpoch,
        w_post_challenge_lookback: ChainEpoch,
        fault_declaration_cutoff: ChainEpoch,
    ) -> Self {
        if deadline_idx < w_post_period_deadlines {
            let deadline_open = period_start + (deadline_idx as i64 * w_post_challenge_window);
            Self {
                current_epoch,
                period_start,
                index: deadline_idx,
                open: deadline_open,
                close: deadline_open + w_post_challenge_window,
                challenge: deadline_open - w_post_challenge_lookback,
                fault_cutoff: deadline_open - fault_declaration_cutoff,
                w_post_period_deadlines,
                w_post_proving_period,
                w_post_challenge_window,
                w_post_challenge_lookback,
                fault_declaration_cutoff,
            }
        } else {
            let after_last_deadline = period_start + w_post_proving_period;
            Self {
                current_epoch,
                period_start,
                index: deadline_idx,
                open: after_last_deadline,
                close: after_last_deadline,
                challenge: after_last_deadline,
                fault_cutoff: 0,
                w_post_period_deadlines,
                w_post_proving_period,
                w_post_challenge_window,
                w_post_challenge_lookback,
                fault_declaration_cutoff,
            }
        }
    }

    /// Whether the proving period has begun.
    pub fn period_started(&self) -> bool {
        self.current_epoch >= self.period_start
    }

    /// Whether the proving period has elapsed.
    pub fn period_elapsed(&self) -> bool {
        self.current_epoch >= self.next_period_start()
    }

    /// The last epoch in the proving period.
    pub fn period_end(&self) -> ChainEpoch {
        self.period_start + self.w_post_proving_period - 1
    }

    /// The first epoch in the next proving period.
    pub fn next_period_start(&self) -> ChainEpoch {
        self.period_start + self.w_post_proving_period
    }

    /// Whether the current deadline is currently open.
    pub fn is_open(&self) -> bool {
        self.current_epoch >= self.open && self.current_epoch < self.close
    }

    /// Whether the current deadline has already closed.
    pub fn has_elapsed(&self) -> bool {
        self.current_epoch >= self.close
    }

    /// The last epoch during which a proof may be submitted.
    pub fn last(&self) -> ChainEpoch {
        self.close - 1
    }

    /// Epoch at which the subsequent deadline opens.
    pub fn next_open(&self) -> ChainEpoch {
        self.close
    }

    /// Whether the deadline's fault cutoff has passed.
    pub fn fault_cutoff_passed(&self) -> bool {
        self.current_epoch >= self.fault_cutoff
    }

    /// Returns the next instance of this deadline that has not yet elapsed.
    pub fn next_not_elapsed(self) -> Self {
        if !self.has_elapsed() {
            return self;
        }

        // has elapsed, advance by some multiples of w_post_proving_period
        let gap = self.current_epoch - self.close;
        let delta_periods = 1 + gap / self.w_post_proving_period;

        Self::new(
            self.period_start + self.w_post_proving_period * delta_periods,
            self.index,
            self.current_epoch,
            self.w_post_period_deadlines,
            self.w_post_proving_period,
            self.w_post_challenge_window,
            self.w_post_challenge_lookback,
            self.fault_declaration_cutoff,
        )
    }

    pub fn quant_spec(&self) -> QuantSpec {
        QuantSpec {
            unit: self.w_post_proving_period,
            offset: self.last(),
        }
    }
}

impl From<fil_actor_miner_state::v8::DeadlineInfo> for DeadlineInfo {
    fn from(info: fil_actor_miner_state::v8::DeadlineInfo) -> Self {
        let fil_actor_miner_state::v8::DeadlineInfo {
            current_epoch,
            period_start,
            index,
            open,
            close,
            challenge,
            fault_cutoff,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        } = info;
        DeadlineInfo {
            current_epoch,
            period_start,
            index,
            open,
            close,
            challenge,
            fault_cutoff,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        }
    }
}

impl From<fil_actor_miner_state::v9::DeadlineInfo> for DeadlineInfo {
    fn from(info: fil_actor_miner_state::v9::DeadlineInfo) -> Self {
        let fil_actor_miner_state::v9::DeadlineInfo {
            current_epoch,
            period_start,
            index,
            open,
            close,
            challenge,
            fault_cutoff,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        } = info;
        DeadlineInfo {
            current_epoch,
            period_start,
            index,
            open,
            close,
            challenge,
            fault_cutoff,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        }
    }
}

impl From<fil_actor_miner_state::v10::DeadlineInfo> for DeadlineInfo {
    fn from(info: fil_actor_miner_state::v10::DeadlineInfo) -> Self {
        let fil_actor_miner_state::v10::DeadlineInfo {
            current_epoch,
            period_start,
            index,
            open,
            close,
            challenge,
            fault_cutoff,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        } = info;
        DeadlineInfo {
            current_epoch,
            period_start,
            index,
            open,
            close,
            challenge,
            fault_cutoff,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        }
    }
}

impl From<fil_actor_miner_state::v11::DeadlineInfo> for DeadlineInfo {
    fn from(info: fil_actor_miner_state::v11::DeadlineInfo) -> Self {
        let fil_actor_miner_state::v11::DeadlineInfo {
            current_epoch,
            period_start,
            index,
            open,
            close,
            challenge,
            fault_cutoff,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        } = info;
        DeadlineInfo {
            current_epoch,
            period_start,
            index,
            open,
            close,
            challenge,
            fault_cutoff,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        }
    }
}

impl From<fil_actor_miner_state::v12::DeadlineInfo> for DeadlineInfo {
    fn from(info: fil_actor_miner_state::v12::DeadlineInfo) -> Self {
        let fil_actor_miner_state::v12::DeadlineInfo {
            current_epoch,
            period_start,
            index,
            open,
            close,
            challenge,
            fault_cutoff,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        } = info;
        DeadlineInfo {
            current_epoch,
            period_start,
            index,
            open,
            close,
            challenge,
            fault_cutoff,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        }
    }
}

impl From<fil_actor_miner_state::v13::DeadlineInfo> for DeadlineInfo {
    fn from(info: fil_actor_miner_state::v13::DeadlineInfo) -> Self {
        let fil_actor_miner_state::v13::DeadlineInfo {
            current_epoch,
            period_start,
            index,
            open,
            close,
            challenge,
            fault_cutoff,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        } = info;
        DeadlineInfo {
            current_epoch,
            period_start,
            index,
            open,
            close,
            challenge,
            fault_cutoff,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        }
    }
}

impl From<fil_actor_miner_state::v14::DeadlineInfo> for DeadlineInfo {
    fn from(info: fil_actor_miner_state::v14::DeadlineInfo) -> Self {
        let fil_actor_miner_state::v14::DeadlineInfo {
            current_epoch,
            period_start,
            index,
            open,
            close,
            challenge,
            fault_cutoff,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        } = info;
        DeadlineInfo {
            current_epoch,
            period_start,
            index,
            open,
            close,
            challenge,
            fault_cutoff,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        }
    }
}

impl From<fil_actor_miner_state::v15::DeadlineInfo> for DeadlineInfo {
    fn from(info: fil_actor_miner_state::v15::DeadlineInfo) -> Self {
        let fil_actor_miner_state::v15::DeadlineInfo {
            current_epoch,
            period_start,
            index,
            open,
            close,
            challenge,
            fault_cutoff,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        } = info;
        DeadlineInfo {
            current_epoch,
            period_start,
            index,
            open,
            close,
            challenge,
            fault_cutoff,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        }
    }
}

impl From<fil_actor_miner_state::v16::DeadlineInfo> for DeadlineInfo {
    fn from(info: fil_actor_miner_state::v16::DeadlineInfo) -> Self {
        let fil_actor_miner_state::v16::DeadlineInfo {
            current_epoch,
            period_start,
            index,
            open,
            close,
            challenge,
            fault_cutoff,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        } = info;
        DeadlineInfo {
            current_epoch,
            period_start,
            index,
            open,
            close,
            challenge,
            fault_cutoff,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        }
    }
}

impl From<fil_actor_miner_state::v17::DeadlineInfo> for DeadlineInfo {
    fn from(info: fil_actor_miner_state::v17::DeadlineInfo) -> Self {
        let fil_actor_miner_state::v17::DeadlineInfo {
            current_epoch,
            period_start,
            index,
            open,
            close,
            challenge,
            fault_cutoff,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        } = info;
        DeadlineInfo {
            current_epoch,
            period_start,
            index,
            open,
            close,
            challenge,
            fault_cutoff,
            w_post_period_deadlines,
            w_post_proving_period,
            w_post_challenge_window,
            w_post_challenge_lookback,
            fault_declaration_cutoff,
        }
    }
}
