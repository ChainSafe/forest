// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod ext;

use crate::shim::actors::Policy;
use crate::shim::actors::convert::*;
use cid::Cid;
use fil_actor_miner_state::v12::{BeneficiaryTerm, PendingBeneficiaryChange};
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{BytesDe, serde_bytes};
use fvm_shared2::{
    address::Address,
    clock::{ChainEpoch, QuantSpec},
    deal::DealID,
    econ::TokenAmount,
    sector::{RegisteredPoStProof, RegisteredSealProof, SectorNumber, SectorSize},
};
use num::BigInt;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

use crate::shim::actors::power::Claim;
/// Miner actor method.
pub type Method = fil_actor_miner_state::v8::Method;

/// Miner actor state.
#[derive(Serialize, Debug)]
#[serde(untagged)]
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
}

impl State {
    pub fn info<BS: Blockstore>(&self, store: &BS) -> anyhow::Result<MinerInfo> {
        match self {
            State::V8(st) => Ok(st.get_info(store)?.into()),
            State::V9(st) => Ok(st.get_info(store)?.into()),
            State::V10(st) => Ok(st.get_info(store)?.into()),
            State::V11(st) => Ok(st.get_info(store)?.into()),
            State::V12(st) => Ok(st.get_info(store)?.into()),
            State::V13(st) => Ok(st.get_info(store)?.into()),
            State::V14(st) => Ok(st.get_info(store)?.into()),
            State::V15(st) => Ok(st.get_info(store)?.into()),
            State::V16(st) => Ok(st.get_info(store)?.into()),
        }
    }

    /// Loads deadlines for a miner's state
    pub fn for_each_deadline<BS: Blockstore>(
        &self,
        policy: &Policy,
        store: &BS,
        mut f: impl FnMut(u64, Deadline) -> Result<(), anyhow::Error>,
    ) -> anyhow::Result<()> {
        match self {
            State::V8(st) => st.load_deadlines(&store)?.for_each(
                &from_policy_v13_to_v9(policy),
                &store,
                |idx, dl| f(idx, Deadline::V8(dl)),
            ),
            State::V9(st) => st.load_deadlines(&store)?.for_each(
                &from_policy_v13_to_v9(policy),
                &store,
                |idx, dl| f(idx, Deadline::V9(dl)),
            ),
            State::V10(st) => st.load_deadlines(&store)?.for_each(
                &from_policy_v13_to_v10(policy),
                &store,
                |idx, dl| f(idx, Deadline::V10(dl)),
            ),
            State::V11(st) => st.load_deadlines(&store)?.for_each(
                &from_policy_v13_to_v11(policy),
                &store,
                |idx, dl| f(idx, Deadline::V11(dl)),
            ),
            State::V12(st) => st
                .load_deadlines(store)?
                .for_each(store, |idx, dl| f(idx, Deadline::V12(dl))),
            State::V13(st) => st
                .load_deadlines(store)?
                .for_each(store, |idx, dl| f(idx, Deadline::V13(dl))),
            State::V14(st) => st
                .load_deadlines(store)?
                .for_each(store, |idx, dl| f(idx, Deadline::V14(dl))),
            State::V15(st) => st
                .load_deadlines(store)?
                .for_each(store, |idx, dl| f(idx, Deadline::V15(dl))),
            State::V16(st) => st
                .load_deadlines(store)?
                .for_each(store, |idx, dl| f(idx, Deadline::V16(dl))),
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
                .load_deadline(&from_policy_v13_to_v9(policy), store, idx)
                .map(Deadline::V8)?),
            State::V9(st) => Ok(st
                .load_deadlines(store)?
                .load_deadline(&from_policy_v13_to_v9(policy), store, idx)
                .map(Deadline::V9)?),
            State::V10(st) => Ok(st
                .load_deadlines(store)?
                .load_deadline(&from_policy_v13_to_v10(policy), store, idx)
                .map(Deadline::V10)?),
            State::V11(st) => Ok(st
                .load_deadlines(store)?
                .load_deadline(&from_policy_v13_to_v11(policy), store, idx)
                .map(Deadline::V11)?),
            State::V12(st) => Ok(st
                .load_deadlines(store)?
                .load_deadline(store, idx)
                .map(Deadline::V12)?),
            State::V13(st) => Ok(st
                .load_deadlines(store)?
                .load_deadline(store, idx)
                .map(Deadline::V13)?),
            State::V14(st) => Ok(st
                .load_deadlines(store)?
                .load_deadline(store, idx)
                .map(Deadline::V14)?),
            State::V15(st) => Ok(st
                .load_deadlines(store)?
                .load_deadline(store, idx)
                .map(Deadline::V15)?),
            State::V16(st) => Ok(st
                .load_deadlines(store)?
                .load_deadline(store, idx)
                .map(Deadline::V16)?),
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
            State::V8(st) => st.find_sector(&from_policy_v13_to_v9(policy), store, sector_number),
            State::V9(st) => st.find_sector(&from_policy_v13_to_v9(policy), store, sector_number),
            State::V10(st) => st.find_sector(&from_policy_v13_to_v10(policy), store, sector_number),
            State::V11(st) => st.find_sector(&from_policy_v13_to_v11(policy), store, sector_number),
            State::V12(st) => st.find_sector(store, sector_number),
            State::V13(st) => st.find_sector(store, sector_number),
            State::V14(st) => st.find_sector(store, sector_number),
            State::V15(st) => st.find_sector(store, sector_number),
            State::V16(st) => st.find_sector(store, sector_number),
        }
    }

    /// Gets fee debt of miner state
    pub fn fee_debt(&self) -> TokenAmount {
        match self {
            State::V8(st) => st.fee_debt.clone(),
            State::V9(st) => st.fee_debt.clone(),
            State::V10(st) => from_token_v3_to_v2(&st.fee_debt),
            State::V11(st) => from_token_v3_to_v2(&st.fee_debt),
            State::V12(st) => from_token_v4_to_v2(&st.fee_debt),
            State::V13(st) => from_token_v4_to_v2(&st.fee_debt),
            State::V14(st) => from_token_v4_to_v2(&st.fee_debt),
            State::V15(st) => from_token_v4_to_v2(&st.fee_debt),
            State::V16(st) => from_token_v4_to_v2(&st.fee_debt),
        }
    }

    /// Unclaimed funds. Actor balance - (locked funds, precommit deposit, ip requirement) Can go negative if the miner is in IP debt.
    pub fn available_balance(&self, balance: &BigInt) -> anyhow::Result<TokenAmount> {
        let balance: TokenAmount = TokenAmount::from_atto(balance.clone());
        let balance_v3 = from_token_v2_to_v3(&balance);
        let balance_v4 = from_token_v2_to_v4(&balance);
        match self {
            State::V8(st) => st.get_available_balance(&balance),
            State::V9(st) => st.get_available_balance(&balance),
            State::V10(st) => Ok(from_token_v3_to_v2(&st.get_available_balance(&balance_v3)?)),
            State::V11(st) => Ok(from_token_v3_to_v2(&st.get_available_balance(&balance_v3)?)),
            State::V12(st) => Ok(from_token_v4_to_v2(&st.get_available_balance(&balance_v4)?)),
            State::V13(st) => Ok(from_token_v4_to_v2(&st.get_available_balance(&balance_v4)?)),
            State::V14(st) => Ok(from_token_v4_to_v2(&st.get_available_balance(&balance_v4)?)),
            State::V15(st) => Ok(from_token_v4_to_v2(&st.get_available_balance(&balance_v4)?)),
            State::V16(st) => Ok(from_token_v4_to_v2(&st.get_available_balance(&balance_v4)?)),
        }
    }

    /// Returns deadline calculations for the current (according to state) proving period.
    pub fn deadline_info(&self, policy: &Policy, current_epoch: ChainEpoch) -> DeadlineInfo {
        match self {
            State::V8(st) => st
                .deadline_info(&from_policy_v13_to_v9(policy), current_epoch)
                .into(),
            State::V9(st) => st
                .deadline_info(&from_policy_v13_to_v9(policy), current_epoch)
                .into(),
            State::V10(st) => st
                .deadline_info(&from_policy_v13_to_v10(policy), current_epoch)
                .into(),
            State::V11(st) => st
                .deadline_info(&from_policy_v13_to_v11(policy), current_epoch)
                .into(),
            State::V12(st) => st
                .deadline_info(&from_policy_v13_to_v12(policy), current_epoch)
                .into(),
            State::V13(st) => st.deadline_info(policy, current_epoch).into(),
            State::V14(st) => st
                .deadline_info(&from_policy_v13_to_v14(policy), current_epoch)
                .into(),
            State::V15(st) => st
                .deadline_info(&from_policy_v13_to_v15(policy), current_epoch)
                .into(),
            State::V16(st) => st
                .deadline_info(&from_policy_v13_to_v16(policy), current_epoch)
                .into(),
        }
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
            owner: info.owner,
            worker: info.worker,
            control_addresses: info.control_addresses.into_iter().collect(),
            new_worker: info.pending_worker_key.as_ref().map(|k| k.new_worker),
            worker_change_epoch: info
                .pending_worker_key
                .map(|k| k.effective_at)
                .unwrap_or(-1),
            peer_id: info.peer_id,
            multiaddrs: info.multi_address,
            window_post_proof_type: info.window_post_proof_type,
            sector_size: info.sector_size,
            window_post_partition_sectors: info.window_post_partition_sectors,
            consensus_fault_elapsed: info.consensus_fault_elapsed,
            pending_owner_address: info.pending_owner_address,
            beneficiary: info.owner,
            beneficiary_term: BeneficiaryTerm::default(),
            pending_beneficiary_term: None,
        }
    }
}

impl From<fil_actor_miner_state::v9::MinerInfo> for MinerInfo {
    fn from(info: fil_actor_miner_state::v9::MinerInfo) -> Self {
        MinerInfo {
            owner: info.owner,
            worker: info.worker,
            control_addresses: info.control_addresses.into_iter().collect(),
            new_worker: info.pending_worker_key.as_ref().map(|k| k.new_worker),
            worker_change_epoch: info
                .pending_worker_key
                .map(|k| k.effective_at)
                .unwrap_or(-1),
            peer_id: info.peer_id,
            multiaddrs: info.multi_address,
            window_post_proof_type: info.window_post_proof_type,
            sector_size: info.sector_size,
            window_post_partition_sectors: info.window_post_partition_sectors,
            consensus_fault_elapsed: info.consensus_fault_elapsed,
            pending_owner_address: info.pending_owner_address,
            beneficiary: info.beneficiary,
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
            owner: from_address_v3_to_v2(info.owner),
            worker: from_address_v3_to_v2(info.worker),
            control_addresses: info
                .control_addresses
                .into_iter()
                .map(from_address_v3_to_v2)
                .collect(),
            new_worker: info
                .pending_worker_key
                .as_ref()
                .map(|k| from_address_v3_to_v2(k.new_worker)),
            worker_change_epoch: info
                .pending_worker_key
                .map(|k| k.effective_at)
                .unwrap_or(-1),
            peer_id: info.peer_id,
            multiaddrs: info.multi_address,
            window_post_proof_type: from_reg_post_proof_v3_to_v2(info.window_post_proof_type),
            sector_size: from_sector_size_v3_to_v2(info.sector_size),
            window_post_partition_sectors: info.window_post_partition_sectors,
            consensus_fault_elapsed: info.consensus_fault_elapsed,
            pending_owner_address: info.pending_owner_address.map(from_address_v3_to_v2),
            beneficiary: from_address_v3_to_v2(info.beneficiary),
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
            owner: from_address_v3_to_v2(info.owner),
            worker: from_address_v3_to_v2(info.worker),
            control_addresses: info
                .control_addresses
                .into_iter()
                .map(from_address_v3_to_v2)
                .collect(),
            new_worker: info
                .pending_worker_key
                .as_ref()
                .map(|k| from_address_v3_to_v2(k.new_worker)),
            worker_change_epoch: info
                .pending_worker_key
                .map(|k| k.effective_at)
                .unwrap_or(-1),
            peer_id: info.peer_id,
            multiaddrs: info.multi_address,
            window_post_proof_type: from_reg_post_proof_v3_to_v2(info.window_post_proof_type),
            sector_size: from_sector_size_v3_to_v2(info.sector_size),
            window_post_partition_sectors: info.window_post_partition_sectors,
            consensus_fault_elapsed: info.consensus_fault_elapsed,
            pending_owner_address: info.pending_owner_address.map(from_address_v3_to_v2),
            beneficiary: from_address_v3_to_v2(info.beneficiary),
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
            owner: from_address_v4_to_v2(info.owner),
            worker: from_address_v4_to_v2(info.worker),
            control_addresses: info
                .control_addresses
                .into_iter()
                .map(from_address_v4_to_v2)
                .collect(),
            new_worker: info
                .pending_worker_key
                .as_ref()
                .map(|k| from_address_v4_to_v2(k.new_worker)),
            worker_change_epoch: info
                .pending_worker_key
                .map(|k| k.effective_at)
                .unwrap_or(-1),
            peer_id: info.peer_id,
            multiaddrs: info.multi_address,
            window_post_proof_type: from_reg_post_proof_v4_to_v2(info.window_post_proof_type),
            sector_size: from_sector_size_v4_to_v2(info.sector_size),
            window_post_partition_sectors: info.window_post_partition_sectors,
            consensus_fault_elapsed: info.consensus_fault_elapsed,
            pending_owner_address: info.pending_owner_address.map(from_address_v4_to_v2),
            beneficiary: from_address_v4_to_v2(info.beneficiary),
            beneficiary_term: info.beneficiary_term,
            pending_beneficiary_term: info.pending_beneficiary_term,
        }
    }
}

impl From<fil_actor_miner_state::v13::MinerInfo> for MinerInfo {
    fn from(info: fil_actor_miner_state::v13::MinerInfo) -> Self {
        MinerInfo {
            owner: from_address_v4_to_v2(info.owner),
            worker: from_address_v4_to_v2(info.worker),
            control_addresses: info
                .control_addresses
                .into_iter()
                .map(from_address_v4_to_v2)
                .collect(),
            new_worker: info
                .pending_worker_key
                .as_ref()
                .map(|k| from_address_v4_to_v2(k.new_worker)),
            worker_change_epoch: info
                .pending_worker_key
                .map(|k| k.effective_at)
                .unwrap_or(-1),
            peer_id: info.peer_id,
            multiaddrs: info.multi_address,
            window_post_proof_type: from_reg_post_proof_v4_to_v2(info.window_post_proof_type),
            sector_size: from_sector_size_v4_to_v2(info.sector_size),
            window_post_partition_sectors: info.window_post_partition_sectors,
            consensus_fault_elapsed: info.consensus_fault_elapsed,
            pending_owner_address: info.pending_owner_address.map(from_address_v4_to_v2),
            beneficiary: from_address_v4_to_v2(info.beneficiary),
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
            owner: from_address_v4_to_v2(info.owner),
            worker: from_address_v4_to_v2(info.worker),
            control_addresses: info
                .control_addresses
                .into_iter()
                .map(from_address_v4_to_v2)
                .collect(),
            new_worker: info
                .pending_worker_key
                .as_ref()
                .map(|k| from_address_v4_to_v2(k.new_worker)),
            worker_change_epoch: info
                .pending_worker_key
                .map(|k| k.effective_at)
                .unwrap_or(-1),
            peer_id: info.peer_id,
            multiaddrs: info.multi_address,
            window_post_proof_type: from_reg_post_proof_v4_to_v2(info.window_post_proof_type),
            sector_size: from_sector_size_v4_to_v2(info.sector_size),
            window_post_partition_sectors: info.window_post_partition_sectors,
            consensus_fault_elapsed: info.consensus_fault_elapsed,
            pending_owner_address: info.pending_owner_address.map(from_address_v4_to_v2),
            beneficiary: from_address_v4_to_v2(info.beneficiary),
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
            owner: from_address_v4_to_v2(info.owner),
            worker: from_address_v4_to_v2(info.worker),
            control_addresses: info
                .control_addresses
                .into_iter()
                .map(from_address_v4_to_v2)
                .collect(),
            new_worker: info
                .pending_worker_key
                .as_ref()
                .map(|k| from_address_v4_to_v2(k.new_worker)),
            worker_change_epoch: info
                .pending_worker_key
                .map(|k| k.effective_at)
                .unwrap_or(-1),
            peer_id: info.peer_id,
            multiaddrs: info.multi_address,
            window_post_proof_type: from_reg_post_proof_v4_to_v2(info.window_post_proof_type),
            sector_size: from_sector_size_v4_to_v2(info.sector_size),
            window_post_partition_sectors: info.window_post_partition_sectors,
            consensus_fault_elapsed: info.consensus_fault_elapsed,
            pending_owner_address: info.pending_owner_address.map(from_address_v4_to_v2),
            beneficiary: from_address_v4_to_v2(info.beneficiary),
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
            owner: from_address_v4_to_v2(info.owner),
            worker: from_address_v4_to_v2(info.worker),
            control_addresses: info
                .control_addresses
                .into_iter()
                .map(from_address_v4_to_v2)
                .collect(),
            new_worker: info
                .pending_worker_key
                .as_ref()
                .map(|k| from_address_v4_to_v2(k.new_worker)),
            worker_change_epoch: info
                .pending_worker_key
                .map(|k| k.effective_at)
                .unwrap_or(-1),
            peer_id: info.peer_id,
            multiaddrs: info.multi_address,
            window_post_proof_type: from_reg_post_proof_v4_to_v2(info.window_post_proof_type),
            sector_size: from_sector_size_v4_to_v2(info.sector_size),
            window_post_partition_sectors: info.window_post_partition_sectors,
            consensus_fault_elapsed: info.consensus_fault_elapsed,
            pending_owner_address: info.pending_owner_address.map(from_address_v4_to_v2),
            beneficiary: from_address_v4_to_v2(info.beneficiary),
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
}

impl Deadline {
    /// For each partition of the deadline
    pub fn for_each<BS: Blockstore>(
        &self,
        store: &BS,
        mut f: impl FnMut(u64, Partition) -> Result<(), anyhow::Error>,
    ) -> anyhow::Result<()> {
        match self {
            Deadline::V8(dl) => dl.for_each(&store, |idx, part| {
                f(idx, Partition::V8(Cow::Borrowed(part)))
            }),
            Deadline::V9(dl) => dl.for_each(&store, |idx, part| {
                f(idx, Partition::V9(Cow::Borrowed(part)))
            }),
            Deadline::V10(dl) => dl.for_each(&store, |idx, part| {
                f(idx, Partition::V10(Cow::Borrowed(part)))
            }),
            Deadline::V11(dl) => dl.for_each(&store, |idx, part| {
                f(idx, Partition::V11(Cow::Borrowed(part)))
            }),
            Deadline::V12(dl) => dl.for_each(&store, |idx, part| {
                f(idx, Partition::V12(Cow::Borrowed(part)))
            }),
            Deadline::V13(dl) => dl.for_each(&store, |idx, part| {
                f(idx, Partition::V13(Cow::Borrowed(part)))
            }),
            Deadline::V14(dl) => dl.for_each(&store, |idx, part| {
                f(idx, Partition::V14(Cow::Borrowed(part)))
            }),
            Deadline::V15(dl) => dl.for_each(&store, |idx, part| {
                f(idx, Partition::V15(Cow::Borrowed(part)))
            }),
            Deadline::V16(dl) => dl.for_each(&store, |idx, part| {
                f(idx, Partition::V16(Cow::Borrowed(part)))
            }),
        }
    }

    /// Returns number of partitions posted
    pub fn partitions_posted(&self) -> BitField {
        match self {
            Deadline::V8(dl) => dl.partitions_posted.clone(),
            Deadline::V9(dl) => dl.partitions_posted.clone(),
            Deadline::V10(dl) => dl.partitions_posted.clone(),
            Deadline::V11(dl) => dl.partitions_posted.clone(),
            Deadline::V12(dl) => dl.partitions_posted.clone(),
            Deadline::V13(dl) => dl.partitions_posted.clone(),
            Deadline::V14(dl) => dl.partitions_posted.clone(),
            Deadline::V15(dl) => dl.partitions_posted.clone(),
            Deadline::V16(dl) => dl.partitions_posted.clone(),
        }
    }

    /// Returns disputable proof count of the deadline
    pub fn disputable_proof_count<BS: Blockstore>(&self, store: &BS) -> anyhow::Result<u64> {
        match self {
            Deadline::V8(dl) => Ok(dl.optimistic_proofs_snapshot_amt(store)?.count()),
            Deadline::V9(dl) => Ok(dl.optimistic_proofs_snapshot_amt(store)?.count()),
            Deadline::V10(dl) => Ok(dl.optimistic_proofs_snapshot_amt(store)?.count()),
            Deadline::V11(dl) => Ok(dl.optimistic_proofs_snapshot_amt(store)?.count()),
            Deadline::V12(dl) => Ok(dl.optimistic_proofs_snapshot_amt(store)?.count()),
            Deadline::V13(dl) => Ok(dl.optimistic_proofs_snapshot_amt(store)?.count()),
            Deadline::V14(dl) => Ok(dl.optimistic_proofs_snapshot_amt(store)?.count()),
            Deadline::V15(dl) => Ok(dl.optimistic_proofs_snapshot_amt(store)?.count()),
            Deadline::V16(dl) => Ok(dl.optimistic_proofs_snapshot_amt(store)?.count()),
        }
    }
}

#[allow(clippy::large_enum_variant)]
pub enum Partition<'a> {
    // V7(Cow<'a, fil_actor_miner_state::v7::Partition>),
    V8(Cow<'a, fil_actor_miner_state::v8::Partition>),
    V9(Cow<'a, fil_actor_miner_state::v9::Partition>),
    V10(Cow<'a, fil_actor_miner_state::v10::Partition>),
    V11(Cow<'a, fil_actor_miner_state::v11::Partition>),
    V12(Cow<'a, fil_actor_miner_state::v12::Partition>),
    V13(Cow<'a, fil_actor_miner_state::v13::Partition>),
    V14(Cow<'a, fil_actor_miner_state::v14::Partition>),
    V15(Cow<'a, fil_actor_miner_state::v15::Partition>),
    V16(Cow<'a, fil_actor_miner_state::v16::Partition>),
}

impl Partition<'_> {
    pub fn all_sectors(&self) -> &BitField {
        match self {
            Partition::V8(dl) => &dl.sectors,
            Partition::V9(dl) => &dl.sectors,
            Partition::V10(dl) => &dl.sectors,
            Partition::V11(dl) => &dl.sectors,
            Partition::V12(dl) => &dl.sectors,
            Partition::V13(dl) => &dl.sectors,
            Partition::V14(dl) => &dl.sectors,
            Partition::V15(dl) => &dl.sectors,
            Partition::V16(dl) => &dl.sectors,
        }
    }
    pub fn faulty_sectors(&self) -> &BitField {
        match self {
            Partition::V8(dl) => &dl.faults,
            Partition::V9(dl) => &dl.faults,
            Partition::V10(dl) => &dl.faults,
            Partition::V11(dl) => &dl.faults,
            Partition::V12(dl) => &dl.faults,
            Partition::V13(dl) => &dl.faults,
            Partition::V14(dl) => &dl.faults,
            Partition::V15(dl) => &dl.faults,
            Partition::V16(dl) => &dl.faults,
        }
    }
    pub fn live_sectors(&self) -> BitField {
        match self {
            Partition::V8(dl) => dl.live_sectors(),
            Partition::V9(dl) => dl.live_sectors(),
            Partition::V10(dl) => dl.live_sectors(),
            Partition::V11(dl) => dl.live_sectors(),
            Partition::V12(dl) => dl.live_sectors(),
            Partition::V13(dl) => dl.live_sectors(),
            Partition::V14(dl) => dl.live_sectors(),
            Partition::V15(dl) => dl.live_sectors(),
            Partition::V16(dl) => dl.live_sectors(),
        }
    }
    pub fn active_sectors(&self) -> BitField {
        match self {
            Partition::V8(dl) => dl.active_sectors(),
            Partition::V9(dl) => dl.active_sectors(),
            Partition::V10(dl) => dl.active_sectors(),
            Partition::V11(dl) => dl.active_sectors(),
            Partition::V12(dl) => dl.active_sectors(),
            Partition::V13(dl) => dl.active_sectors(),
            Partition::V14(dl) => dl.active_sectors(),
            Partition::V15(dl) => dl.active_sectors(),
            Partition::V16(dl) => dl.active_sectors(),
        }
    }
    pub fn recovering_sectors(&self) -> &BitField {
        match self {
            Partition::V8(dl) => &dl.recoveries,
            Partition::V9(dl) => &dl.recoveries,
            Partition::V10(dl) => &dl.recoveries,
            Partition::V11(dl) => &dl.recoveries,
            Partition::V12(dl) => &dl.recoveries,
            Partition::V13(dl) => &dl.recoveries,
            Partition::V14(dl) => &dl.recoveries,
            Partition::V15(dl) => &dl.recoveries,
            Partition::V16(dl) => &dl.recoveries,
        }
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
            initial_pledge: info.initial_pledge,
            expected_day_reward: info.expected_day_reward,
            expected_storage_pledge: info.expected_storage_pledge,
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
            initial_pledge: info.initial_pledge,
            expected_day_reward: info.expected_day_reward,
            expected_storage_pledge: info.expected_storage_pledge,
            replaced_sector_age: info.replaced_sector_age,
            replaced_day_reward: info.replaced_day_reward,
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
            initial_pledge: from_token_v3_to_v2(&info.initial_pledge),
            expected_day_reward: from_token_v3_to_v2(&info.expected_day_reward),
            expected_storage_pledge: from_token_v3_to_v2(&info.expected_storage_pledge),
            replaced_sector_age: info.replaced_sector_age,
            replaced_day_reward: from_token_v3_to_v2(&info.replaced_day_reward),
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
            initial_pledge: from_token_v3_to_v2(&info.initial_pledge),
            expected_day_reward: from_token_v3_to_v2(&info.expected_day_reward),
            expected_storage_pledge: from_token_v3_to_v2(&info.expected_storage_pledge),
            replaced_sector_age: info.replaced_sector_age,
            replaced_day_reward: from_token_v3_to_v2(&info.replaced_day_reward),
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
            initial_pledge: from_token_v4_to_v2(&info.initial_pledge),
            expected_day_reward: from_token_v4_to_v2(&info.expected_day_reward),
            expected_storage_pledge: from_token_v4_to_v2(&info.expected_storage_pledge),
            replaced_sector_age: ChainEpoch::default(),
            replaced_day_reward: from_token_v4_to_v2(&info.replaced_day_reward),
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
            initial_pledge: from_token_v4_to_v2(&info.initial_pledge),
            expected_day_reward: from_token_v4_to_v2(&info.expected_day_reward),
            expected_storage_pledge: from_token_v4_to_v2(&info.expected_storage_pledge),
            replaced_sector_age: ChainEpoch::default(),
            replaced_day_reward: from_token_v4_to_v2(&info.replaced_day_reward),
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
            initial_pledge: from_token_v4_to_v2(&info.initial_pledge),
            expected_day_reward: from_token_v4_to_v2(&info.expected_day_reward),
            expected_storage_pledge: from_token_v4_to_v2(&info.expected_storage_pledge),
            replaced_sector_age: ChainEpoch::default(),
            replaced_day_reward: from_token_v4_to_v2(&info.replaced_day_reward),
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
            initial_pledge: from_token_v4_to_v2(&info.initial_pledge),
            expected_day_reward: from_token_v4_to_v2(&info.expected_day_reward),
            expected_storage_pledge: from_token_v4_to_v2(&info.expected_storage_pledge),
            replaced_sector_age: ChainEpoch::default(),
            replaced_day_reward: from_token_v4_to_v2(&info.replaced_day_reward),
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
            initial_pledge: from_token_v4_to_v2(&info.initial_pledge),
            expected_day_reward: from_opt_token_v4_to_v2(&info.expected_day_reward),
            expected_storage_pledge: from_opt_token_v4_to_v2(&info.expected_storage_pledge),
            replaced_sector_age: ChainEpoch::default(),
            replaced_day_reward: from_opt_token_v4_to_v2(&info.replaced_day_reward),
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
