// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod ext;

use crate::shim::{address::Address, clock::ChainEpoch, econ::TokenAmount, piece::PaddedPieceSize};
use cid::Cid;
use fil_actor_market_state::v8::balance_table::BalanceTable as V8BalanceTable;
use fil_actor_market_state::v9::DealArray as V9DealArray;
use fil_actor_market_state::v9::DealMetaArray as V9DealMetaArray;
use fil_actor_market_state::v9::balance_table::BalanceTable as V9BalanceTable;
use fil_actor_market_state::v10::DealArray as V10DealArray;
use fil_actor_market_state::v10::DealMetaArray as V10DealMetaArray;
use fil_actor_market_state::v10::balance_table::BalanceTable as V10BalanceTable;
use fil_actor_market_state::v11::DealArray as V11DealArray;
use fil_actor_market_state::v11::DealMetaArray as V11DealMetaArray;
use fil_actor_market_state::v11::balance_table::BalanceTable as V11BalanceTable;
use fil_actor_market_state::v12::DealArray as V12DealArray;
use fil_actor_market_state::v12::DealMetaArray as V12DealMetaArray;
use fil_actor_market_state::v12::balance_table::BalanceTable as V12BalanceTable;
use fil_actor_market_state::v13::DealArray as V13DealArray;
use fil_actor_market_state::v13::DealMetaArray as V13DealMetaArray;
use fil_actor_market_state::v13::balance_table::BalanceTable as V13BalanceTable;
use fil_actor_market_state::v14::DealArray as V14DealArray;
use fil_actor_market_state::v14::DealMetaArray as V14DealMetaArray;
use fil_actor_market_state::v14::balance_table::BalanceTable as V14BalanceTable;
use fil_actor_market_state::v15::DealArray as V15DealArray;
use fil_actor_market_state::v15::DealMetaArray as V15DealMetaArray;
use fil_actor_market_state::v15::balance_table::BalanceTable as V15BalanceTable;
use fil_actor_market_state::v16::DealArray as V16DealArray;
use fil_actor_market_state::v16::DealMetaArray as V16DealMetaArray;
use fil_actor_market_state::v16::balance_table::BalanceTable as V16BalanceTable;
use fil_actor_market_state::v17::DealArray as V17DealArray;
use fil_actor_market_state::v17::DealMetaArray as V17DealMetaArray;
use fil_actor_market_state::v17::balance_table::BalanceTable as V17BalanceTable;
use fil_actors_shared::v9::AsActorError as V9AsActorError;
use fil_actors_shared::v10::{AsActorError as V10AsActorError, DealWeight};
use fil_actors_shared::v11::AsActorError as V11AsActorError;
use fil_actors_shared::v12::AsActorError as V12AsActorError;
use fil_actors_shared::v13::AsActorError as V13AsActorError;
use fil_actors_shared::v14::AsActorError as V14AsActorError;
use fil_actors_shared::v15::AsActorError as V15AsActorError;
use fil_actors_shared::v16::AsActorError as V16AsActorError;
use fil_actors_shared::v17::AsActorError as V17AsActorError;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared2::error::ExitCode as FVMExitCode;
use fvm_shared3::error::ExitCode as FVM3ExitCode;
use fvm_shared4::error::ExitCode as FVM4ExitCode;
use serde::{Deserialize, Serialize};
use spire_enum::prelude::delegated_enum;

/// Market actor address.
pub const ADDRESS: Address = Address::new_id(5);

/// Market actor method.
pub type Method = fil_actor_market_state::v8::Method;

pub type AllocationID = u64;

/// Market actor state.
#[derive(Serialize, Debug)]
#[serde(untagged)]
pub enum State {
    V8(fil_actor_market_state::v8::State),
    V9(fil_actor_market_state::v9::State),
    V10(fil_actor_market_state::v10::State),
    V11(fil_actor_market_state::v11::State),
    V12(fil_actor_market_state::v12::State),
    V13(fil_actor_market_state::v13::State),
    V14(fil_actor_market_state::v14::State),
    V15(fil_actor_market_state::v15::State),
    V16(fil_actor_market_state::v16::State),
    V17(fil_actor_market_state::v17::State),
}

impl State {
    #[allow(clippy::too_many_arguments)]
    pub fn default_latest_version(
        proposals: Cid,
        states: Cid,
        pending_proposals: Cid,
        escrow_table: Cid,
        locked_table: Cid,
        next_id: u64,
        deal_ops_by_epoch: Cid,
        last_cron: ChainEpoch,
        total_client_locked_collateral: fvm_shared4::econ::TokenAmount,
        total_provider_locked_collateral: fvm_shared4::econ::TokenAmount,
        total_client_storage_fee: fvm_shared4::econ::TokenAmount,
        pending_deal_allocation_ids: Cid,
        provider_sectors: Cid,
    ) -> Self {
        State::V17(fil_actor_market_state::v17::State {
            proposals,
            states,
            pending_proposals,
            escrow_table,
            locked_table,
            next_id,
            deal_ops_by_epoch,
            last_cron,
            total_client_locked_collateral,
            total_provider_locked_collateral,
            total_client_storage_fee,
            pending_deal_allocation_ids,
            provider_sectors,
        })
    }

    /// Loads escrow table
    pub fn escrow_table<'bs, BS>(&self, store: &'bs BS) -> anyhow::Result<BalanceTable<'bs, BS>>
    where
        BS: Blockstore,
    {
        Ok(match self {
            Self::V8(s) => V8BalanceTable::from_root(store, &s.escrow_table)?.into(),
            Self::V9(s) => V9BalanceTable::from_root(store, &s.escrow_table)?.into(),
            Self::V10(s) => V10BalanceTable::from_root(store, &s.escrow_table)?.into(),
            Self::V11(s) => V11BalanceTable::from_root(store, &s.escrow_table)?.into(),
            Self::V12(s) => {
                V12BalanceTable::from_root(store, &s.escrow_table, "escrow table")?.into()
            }
            Self::V13(s) => {
                V13BalanceTable::from_root(store, &s.escrow_table, "escrow table")?.into()
            }
            Self::V14(s) => {
                V14BalanceTable::from_root(store, &s.escrow_table, "escrow table")?.into()
            }
            Self::V15(s) => {
                V15BalanceTable::from_root(store, &s.escrow_table, "escrow table")?.into()
            }
            Self::V16(s) => {
                V16BalanceTable::from_root(store, &s.escrow_table, "escrow table")?.into()
            }
            Self::V17(s) => {
                V16BalanceTable::from_root(store, &s.escrow_table, "escrow table")?.into()
            }
        })
    }

    /// Loads locked funds table
    pub fn locked_table<'bs, BS>(&self, store: &'bs BS) -> anyhow::Result<BalanceTable<'bs, BS>>
    where
        BS: Blockstore,
    {
        Ok(match self {
            Self::V8(s) => V8BalanceTable::from_root(store, &s.locked_table)?.into(),
            Self::V9(s) => V9BalanceTable::from_root(store, &s.locked_table)?.into(),
            Self::V10(s) => V10BalanceTable::from_root(store, &s.locked_table)?.into(),
            Self::V11(s) => V11BalanceTable::from_root(store, &s.locked_table)?.into(),
            Self::V12(s) => {
                V12BalanceTable::from_root(store, &s.locked_table, "locked table")?.into()
            }
            Self::V13(s) => {
                V13BalanceTable::from_root(store, &s.locked_table, "locked table")?.into()
            }
            Self::V14(s) => {
                V14BalanceTable::from_root(store, &s.locked_table, "locked table")?.into()
            }
            Self::V15(s) => {
                V15BalanceTable::from_root(store, &s.locked_table, "locked table")?.into()
            }
            Self::V16(s) => {
                V16BalanceTable::from_root(store, &s.locked_table, "locked table")?.into()
            }
            Self::V17(s) => {
                V17BalanceTable::from_root(store, &s.locked_table, "locked table")?.into()
            }
        })
    }

    /// Deal proposals
    pub fn proposals<'bs, BS>(&'bs self, store: &'bs BS) -> anyhow::Result<DealProposals<'bs, BS>>
    where
        BS: Blockstore,
    {
        match self {
            // `get_proposal_array` does not exist for V8 and V9
            State::V8(_) | State::V9(_) => anyhow::bail!("unimplemented"),
            State::V10(st) => Ok(DealProposals::V10(st.get_proposal_array(store)?)),
            State::V11(st) => Ok(DealProposals::V11(st.get_proposal_array(store)?)),
            State::V12(st) => Ok(DealProposals::V12(st.get_proposal_array(store)?)),
            State::V13(st) => Ok(DealProposals::V13(st.load_proposals(store)?)),
            State::V14(st) => Ok(DealProposals::V14(st.load_proposals(store)?)),
            State::V15(st) => Ok(DealProposals::V15(st.load_proposals(store)?)),
            State::V16(st) => Ok(DealProposals::V16(st.load_proposals(store)?)),
            State::V17(st) => Ok(DealProposals::V17(st.load_proposals(store)?)),
        }
    }

    /// Deal proposal meta data.
    pub fn states<'bs, BS>(&self, store: &'bs BS) -> anyhow::Result<DealStates<'bs, BS>>
    where
        BS: Blockstore,
    {
        match self {
            // `DealMetaArray::load` does not exist for V8
            State::V8(_st) => anyhow::bail!("unimplemented"),
            State::V9(st) => Ok(DealStates::V9(V9AsActorError::context_code(
                V9DealMetaArray::load(&st.states, store),
                FVMExitCode::USR_ILLEGAL_STATE,
                "failed to load deal state array",
            )?)),
            State::V10(st) => Ok(DealStates::V10(V10AsActorError::context_code(
                V10DealMetaArray::load(&st.states, store),
                FVM3ExitCode::USR_ILLEGAL_STATE,
                "failed to load deal state array",
            )?)),
            State::V11(st) => Ok(DealStates::V11(V11AsActorError::context_code(
                V11DealMetaArray::load(&st.states, store),
                FVM3ExitCode::USR_ILLEGAL_STATE,
                "failed to load deal state array",
            )?)),
            State::V12(st) => Ok(DealStates::V12(V12AsActorError::context_code(
                V12DealMetaArray::load(&st.states, store),
                FVM4ExitCode::USR_ILLEGAL_STATE,
                "failed to load deal state array",
            )?)),
            State::V13(st) => Ok(DealStates::V13(V13AsActorError::context_code(
                V13DealMetaArray::load(&st.states, store),
                FVM4ExitCode::USR_ILLEGAL_STATE,
                "failed to load deal state array",
            )?)),
            State::V14(st) => Ok(DealStates::V14(V14AsActorError::context_code(
                V14DealMetaArray::load(&st.states, store),
                FVM4ExitCode::USR_ILLEGAL_STATE,
                "failed to load deal state array",
            )?)),
            State::V15(st) => Ok(DealStates::V15(V15AsActorError::context_code(
                V15DealMetaArray::load(&st.states, store),
                FVM4ExitCode::USR_ILLEGAL_STATE,
                "failed to load deal state array",
            )?)),
            State::V16(st) => Ok(DealStates::V16(V16AsActorError::context_code(
                V16DealMetaArray::load(&st.states, store),
                FVM4ExitCode::USR_ILLEGAL_STATE,
                "failed to load deal state array",
            )?)),
            State::V17(st) => Ok(DealStates::V17(V16AsActorError::context_code(
                V17DealMetaArray::load(&st.states, store),
                FVM4ExitCode::USR_ILLEGAL_STATE,
                "failed to load deal state array",
            )?)),
        }
    }

    /// Consume state to return just total funds locked
    pub fn total_locked(&self) -> TokenAmount {
        match self {
            State::V8(st) => st.total_locked().into(),
            State::V9(st) => st.total_locked().into(),
            State::V10(st) => st.get_total_locked().into(),
            State::V11(st) => st.get_total_locked().into(),
            State::V12(st) => st.get_total_locked().into(),
            State::V13(st) => st.get_total_locked().into(),
            State::V14(st) => st.get_total_locked().into(),
            State::V15(st) => st.get_total_locked().into(),
            State::V16(st) => st.get_total_locked().into(),
            State::V17(st) => st.get_total_locked().into(),
        }
    }

    pub fn verify_deals_for_activation<BS>(
        &self,
        store: &BS,
        addr: Address,
        deal_ids: Vec<u64>,
        curr_epoch: ChainEpoch,
        sector_exp: i64,
    ) -> anyhow::Result<(DealWeight, DealWeight)>
    where
        BS: Blockstore,
    {
        match self {
            State::V8(_st) => anyhow::bail!("unimplemented"),
            State::V9(_st) => anyhow::bail!("unimplemented"),
            State::V10(st) => Ok(st.verify_deals_for_activation(
                store,
                &addr.into(),
                deal_ids,
                curr_epoch,
                sector_exp,
            )?),
            State::V11(st) => Ok(st.verify_deals_for_activation(
                store,
                &addr.into(),
                deal_ids,
                curr_epoch,
                sector_exp,
            )?),
            State::V12(st) => Ok(st.verify_deals_for_activation(
                store,
                &addr.into(),
                deal_ids,
                curr_epoch,
                sector_exp,
            )?),
            State::V13(st) => Ok(st.verify_deals_for_activation(
                store,
                &addr.into(),
                deal_ids,
                curr_epoch,
                sector_exp,
            )?),
            State::V14(st) => Ok(st.verify_deals_for_activation(
                store,
                &addr.into(),
                deal_ids,
                curr_epoch,
                sector_exp,
            )?),
            State::V15(st) => Ok(st.verify_deals_for_activation(
                store,
                &addr.into(),
                deal_ids,
                curr_epoch,
                sector_exp,
            )?),
            State::V16(st) => Ok(st.verify_deals_for_activation(
                store,
                &addr.into(),
                deal_ids,
                curr_epoch,
                sector_exp,
            )?),
            State::V17(st) => Ok(st.verify_deals_for_activation(
                store,
                &addr.into(),
                deal_ids,
                curr_epoch,
                sector_exp,
            )?),
        }
    }
}

#[delegated_enum(impl_conversions)]
pub enum BalanceTable<'bs, BS: Blockstore> {
    V8(V8BalanceTable<'bs, BS>),
    V9(V9BalanceTable<'bs, BS>),
    V10(V10BalanceTable<'bs, BS>),
    V11(V11BalanceTable<'bs, BS>),
    V12(V12BalanceTable<&'bs BS>),
    V13(V13BalanceTable<&'bs BS>),
    V14(V14BalanceTable<&'bs BS>),
    V15(V15BalanceTable<&'bs BS>),
    V16(V16BalanceTable<&'bs BS>),
    V17(V17BalanceTable<&'bs BS>),
}

#[delegated_enum(impl_conversions)]
pub enum DealProposals<'bs, BS> {
    V9(V9DealArray<'bs, BS>),
    V10(V10DealArray<'bs, BS>),
    V11(V11DealArray<'bs, BS>),
    V12(V12DealArray<'bs, BS>),
    V13(V13DealArray<'bs, BS>),
    V14(V14DealArray<'bs, BS>),
    V15(V15DealArray<'bs, BS>),
    V16(V16DealArray<'bs, BS>),
    V17(V17DealArray<'bs, BS>),
}

impl<BS> DealProposals<'_, BS>
where
    BS: Blockstore,
{
    pub fn for_each(
        &self,
        mut f: impl FnMut(u64, Result<DealProposal, anyhow::Error>) -> anyhow::Result<(), anyhow::Error>,
    ) -> anyhow::Result<()> {
        delegate_deal_proposals!(self => |deal_array| Ok(deal_array
                .for_each(|key, deal_proposal| f(key, deal_proposal.try_into()))?))
    }

    pub fn get(&self, key: u64) -> anyhow::Result<Option<DealProposal>> {
        delegate_deal_proposals!(self.get(key)?.map(TryFrom::try_from).transpose())
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct DealProposal {
    #[serde(rename = "PieceCID")]
    pub piece_cid: Cid,
    pub piece_size: PaddedPieceSize,
    pub verified_deal: bool,
    pub client: Address,
    pub provider: Address,
    // ! This is the field that requires unsafe unchecked utf8 deserialization
    pub label: String,
    pub start_epoch: ChainEpoch,
    pub end_epoch: ChainEpoch,
    pub storage_price_per_epoch: TokenAmount,
    pub provider_collateral: TokenAmount,
    pub client_collateral: TokenAmount,
}

impl TryFrom<&fil_actor_market_state::v9::DealProposal> for DealProposal {
    type Error = anyhow::Error;

    fn try_from(
        deal_proposal: &fil_actor_market_state::v9::DealProposal,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            piece_cid: deal_proposal.piece_cid,
            piece_size: deal_proposal.piece_size.into(),
            verified_deal: deal_proposal.verified_deal,
            client: deal_proposal.client.into(),
            provider: deal_proposal.provider.into(),
            label: match &deal_proposal.label {
                fil_actor_market_state::v9::Label::String(s) => s.clone(),
                fil_actor_market_state::v9::Label::Bytes(b) if b.is_empty() => Default::default(),
                fil_actor_market_state::v9::Label::Bytes(b) => {
                    String::from_utf8(b.clone()).unwrap_or_default()
                }
            },
            start_epoch: deal_proposal.start_epoch,
            end_epoch: deal_proposal.end_epoch,
            storage_price_per_epoch: deal_proposal.storage_price_per_epoch.clone().into(),
            provider_collateral: deal_proposal.provider_collateral.clone().into(),
            client_collateral: deal_proposal.client_collateral.clone().into(),
        })
    }
}

impl TryFrom<&fil_actor_market_state::v10::DealProposal> for DealProposal {
    type Error = anyhow::Error;

    fn try_from(
        deal_proposal: &fil_actor_market_state::v10::DealProposal,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            piece_cid: deal_proposal.piece_cid,
            piece_size: deal_proposal.piece_size.into(),
            verified_deal: deal_proposal.verified_deal,
            client: deal_proposal.client.into(),
            provider: deal_proposal.provider.into(),
            label: match &deal_proposal.label {
                fil_actor_market_state::v10::Label::String(s) => s.clone(),
                fil_actor_market_state::v10::Label::Bytes(b) if b.is_empty() => Default::default(),
                fil_actor_market_state::v10::Label::Bytes(b) => {
                    String::from_utf8(b.clone()).unwrap_or_default()
                }
            },
            start_epoch: deal_proposal.start_epoch,
            end_epoch: deal_proposal.end_epoch,
            storage_price_per_epoch: deal_proposal.storage_price_per_epoch.clone().into(),
            provider_collateral: deal_proposal.provider_collateral.clone().into(),
            client_collateral: deal_proposal.client_collateral.clone().into(),
        })
    }
}

impl TryFrom<&fil_actor_market_state::v11::DealProposal> for DealProposal {
    type Error = anyhow::Error;

    fn try_from(
        deal_proposal: &fil_actor_market_state::v11::DealProposal,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            piece_cid: deal_proposal.piece_cid,
            piece_size: deal_proposal.piece_size.into(),
            verified_deal: deal_proposal.verified_deal,
            client: deal_proposal.client.into(),
            provider: deal_proposal.provider.into(),
            label: match &deal_proposal.label {
                fil_actor_market_state::v11::Label::String(s) => s.clone(),
                fil_actor_market_state::v11::Label::Bytes(b) if b.is_empty() => Default::default(),
                fil_actor_market_state::v11::Label::Bytes(b) => {
                    String::from_utf8(b.clone()).unwrap_or_default()
                }
            },
            start_epoch: deal_proposal.start_epoch,
            end_epoch: deal_proposal.end_epoch,
            storage_price_per_epoch: deal_proposal.storage_price_per_epoch.clone().into(),
            provider_collateral: deal_proposal.provider_collateral.clone().into(),
            client_collateral: deal_proposal.client_collateral.clone().into(),
        })
    }
}

impl TryFrom<&fil_actor_market_state::v12::DealProposal> for DealProposal {
    type Error = anyhow::Error;

    fn try_from(
        deal_proposal: &fil_actor_market_state::v12::DealProposal,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            piece_cid: deal_proposal.piece_cid,
            piece_size: deal_proposal.piece_size.into(),
            verified_deal: deal_proposal.verified_deal,
            client: deal_proposal.client.into(),
            provider: deal_proposal.provider.into(),
            label: match &deal_proposal.label {
                fil_actor_market_state::v12::Label::String(s) => s.clone(),
                fil_actor_market_state::v12::Label::Bytes(b) if b.is_empty() => Default::default(),
                fil_actor_market_state::v12::Label::Bytes(b) => {
                    String::from_utf8(b.clone()).unwrap_or_default()
                }
            },
            start_epoch: deal_proposal.start_epoch,
            end_epoch: deal_proposal.end_epoch,
            storage_price_per_epoch: deal_proposal.storage_price_per_epoch.clone().into(),
            provider_collateral: deal_proposal.provider_collateral.clone().into(),
            client_collateral: deal_proposal.client_collateral.clone().into(),
        })
    }
}

impl TryFrom<&fil_actor_market_state::v13::DealProposal> for DealProposal {
    type Error = anyhow::Error;

    fn try_from(
        deal_proposal: &fil_actor_market_state::v13::DealProposal,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            piece_cid: deal_proposal.piece_cid,
            piece_size: deal_proposal.piece_size.into(),
            verified_deal: deal_proposal.verified_deal,
            client: deal_proposal.client.into(),
            provider: deal_proposal.provider.into(),
            label: match &deal_proposal.label {
                fil_actor_market_state::v13::Label::String(s) => s.clone(),
                fil_actor_market_state::v13::Label::Bytes(b) if b.is_empty() => Default::default(),
                fil_actor_market_state::v13::Label::Bytes(b) => {
                    String::from_utf8(b.clone()).unwrap_or_default()
                }
            },
            start_epoch: deal_proposal.start_epoch,
            end_epoch: deal_proposal.end_epoch,
            storage_price_per_epoch: deal_proposal.storage_price_per_epoch.clone().into(),
            provider_collateral: deal_proposal.provider_collateral.clone().into(),
            client_collateral: deal_proposal.client_collateral.clone().into(),
        })
    }
}

impl TryFrom<&fil_actor_market_state::v14::DealProposal> for DealProposal {
    type Error = anyhow::Error;

    fn try_from(
        deal_proposal: &fil_actor_market_state::v14::DealProposal,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            piece_cid: deal_proposal.piece_cid,
            piece_size: deal_proposal.piece_size.into(),
            verified_deal: deal_proposal.verified_deal,
            client: deal_proposal.client.into(),
            provider: deal_proposal.provider.into(),
            label: match &deal_proposal.label {
                fil_actor_market_state::v14::Label::String(s) => s.clone(),
                fil_actor_market_state::v14::Label::Bytes(b) if b.is_empty() => Default::default(),
                fil_actor_market_state::v14::Label::Bytes(b) => {
                    String::from_utf8(b.clone()).unwrap_or_default()
                }
            },
            start_epoch: deal_proposal.start_epoch,
            end_epoch: deal_proposal.end_epoch,
            storage_price_per_epoch: deal_proposal.storage_price_per_epoch.clone().into(),
            provider_collateral: deal_proposal.provider_collateral.clone().into(),
            client_collateral: deal_proposal.client_collateral.clone().into(),
        })
    }
}

impl TryFrom<&fil_actor_market_state::v15::DealProposal> for DealProposal {
    type Error = anyhow::Error;

    fn try_from(
        deal_proposal: &fil_actor_market_state::v15::DealProposal,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            piece_cid: deal_proposal.piece_cid,
            piece_size: deal_proposal.piece_size.into(),
            verified_deal: deal_proposal.verified_deal,
            client: deal_proposal.client.into(),
            provider: deal_proposal.provider.into(),
            label: match &deal_proposal.label {
                fil_actor_market_state::v15::Label::String(s) => s.clone(),
                fil_actor_market_state::v15::Label::Bytes(b) if b.is_empty() => Default::default(),
                fil_actor_market_state::v15::Label::Bytes(b) => {
                    String::from_utf8(b.clone()).unwrap_or_default()
                }
            },
            start_epoch: deal_proposal.start_epoch,
            end_epoch: deal_proposal.end_epoch,
            storage_price_per_epoch: deal_proposal.storage_price_per_epoch.clone().into(),
            provider_collateral: deal_proposal.provider_collateral.clone().into(),
            client_collateral: deal_proposal.client_collateral.clone().into(),
        })
    }
}

impl TryFrom<&fil_actor_market_state::v16::DealProposal> for DealProposal {
    type Error = anyhow::Error;

    fn try_from(
        deal_proposal: &fil_actor_market_state::v16::DealProposal,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            piece_cid: deal_proposal.piece_cid,
            piece_size: deal_proposal.piece_size.into(),
            verified_deal: deal_proposal.verified_deal,
            client: deal_proposal.client.into(),
            provider: deal_proposal.provider.into(),
            label: match &deal_proposal.label {
                fil_actor_market_state::v16::Label::String(s) => s.clone(),
                fil_actor_market_state::v16::Label::Bytes(b) if b.is_empty() => Default::default(),
                fil_actor_market_state::v16::Label::Bytes(b) => {
                    String::from_utf8(b.clone()).unwrap_or_default()
                }
            },
            start_epoch: deal_proposal.start_epoch,
            end_epoch: deal_proposal.end_epoch,
            storage_price_per_epoch: deal_proposal.storage_price_per_epoch.clone().into(),
            provider_collateral: deal_proposal.provider_collateral.clone().into(),
            client_collateral: deal_proposal.client_collateral.clone().into(),
        })
    }
}

impl TryFrom<&fil_actor_market_state::v17::DealProposal> for DealProposal {
    type Error = anyhow::Error;

    fn try_from(
        deal_proposal: &fil_actor_market_state::v17::DealProposal,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            piece_cid: deal_proposal.piece_cid,
            piece_size: deal_proposal.piece_size.into(),
            verified_deal: deal_proposal.verified_deal,
            client: deal_proposal.client.into(),
            provider: deal_proposal.provider.into(),
            label: match &deal_proposal.label {
                fil_actor_market_state::v17::Label::String(s) => s.clone(),
                fil_actor_market_state::v17::Label::Bytes(b) if b.is_empty() => Default::default(),
                fil_actor_market_state::v17::Label::Bytes(b) => {
                    String::from_utf8(b.clone()).unwrap_or_default()
                }
            },
            start_epoch: deal_proposal.start_epoch,
            end_epoch: deal_proposal.end_epoch,
            storage_price_per_epoch: deal_proposal.storage_price_per_epoch.clone().into(),
            provider_collateral: deal_proposal.provider_collateral.clone().into(),
            client_collateral: deal_proposal.client_collateral.clone().into(),
        })
    }
}

pub enum DealStates<'bs, BS> {
    V8(V9DealMetaArray<'bs, BS>),
    V9(V9DealMetaArray<'bs, BS>),
    V10(V10DealMetaArray<'bs, BS>),
    V11(V11DealMetaArray<'bs, BS>),
    V12(V12DealMetaArray<'bs, BS>),
    V13(V13DealMetaArray<'bs, BS>),
    V14(V14DealMetaArray<'bs, BS>),
    V15(V15DealMetaArray<'bs, BS>),
    V16(V16DealMetaArray<'bs, BS>),
    V17(V17DealMetaArray<'bs, BS>),
}

impl<BS> DealStates<'_, BS>
where
    BS: Blockstore,
{
    pub fn get(&self, key: u64) -> anyhow::Result<Option<DealState>> {
        match self {
            DealStates::V8(deal_array) => Ok(deal_array.get(key)?.map(|deal_state| DealState {
                sector_start_epoch: deal_state.sector_start_epoch,
                last_updated_epoch: deal_state.last_updated_epoch,
                slash_epoch: deal_state.slash_epoch,
                verified_claim: deal_state.verified_claim,
                sector_number: 0,
            })),
            DealStates::V9(deal_array) => Ok(deal_array.get(key)?.map(|deal_state| DealState {
                sector_start_epoch: deal_state.sector_start_epoch,
                last_updated_epoch: deal_state.last_updated_epoch,
                slash_epoch: deal_state.slash_epoch,
                verified_claim: deal_state.verified_claim,
                sector_number: 0,
            })),
            DealStates::V10(deal_array) => Ok(deal_array.get(key)?.map(|deal_state| DealState {
                sector_start_epoch: deal_state.sector_start_epoch,
                last_updated_epoch: deal_state.last_updated_epoch,
                slash_epoch: deal_state.slash_epoch,
                verified_claim: deal_state.verified_claim,
                sector_number: 0,
            })),
            DealStates::V11(deal_array) => Ok(deal_array.get(key)?.map(|deal_state| DealState {
                sector_start_epoch: deal_state.sector_start_epoch,
                last_updated_epoch: deal_state.last_updated_epoch,
                slash_epoch: deal_state.slash_epoch,
                verified_claim: deal_state.verified_claim,
                sector_number: 0,
            })),
            DealStates::V12(deal_array) => Ok(deal_array.get(key)?.map(|deal_state| DealState {
                sector_start_epoch: deal_state.sector_start_epoch,
                last_updated_epoch: deal_state.last_updated_epoch,
                slash_epoch: deal_state.slash_epoch,
                verified_claim: deal_state.verified_claim,
                sector_number: 0,
            })),
            DealStates::V13(deal_array) => Ok(deal_array.get(key)?.map(|deal_state| DealState {
                sector_start_epoch: deal_state.sector_start_epoch,
                last_updated_epoch: deal_state.last_updated_epoch,
                slash_epoch: deal_state.slash_epoch,
                verified_claim: 0,
                sector_number: deal_state.sector_number,
            })),
            DealStates::V14(deal_array) => Ok(deal_array.get(key)?.map(|deal_state| DealState {
                sector_start_epoch: deal_state.sector_start_epoch,
                last_updated_epoch: deal_state.last_updated_epoch,
                slash_epoch: deal_state.slash_epoch,
                verified_claim: 0,
                sector_number: deal_state.sector_number,
            })),
            DealStates::V15(deal_array) => Ok(deal_array.get(key)?.map(|deal_state| DealState {
                sector_start_epoch: deal_state.sector_start_epoch,
                last_updated_epoch: deal_state.last_updated_epoch,
                slash_epoch: deal_state.slash_epoch,
                verified_claim: 0,
                sector_number: deal_state.sector_number,
            })),
            DealStates::V16(deal_array) => Ok(deal_array.get(key)?.map(|deal_state| DealState {
                sector_start_epoch: deal_state.sector_start_epoch,
                last_updated_epoch: deal_state.last_updated_epoch,
                slash_epoch: deal_state.slash_epoch,
                verified_claim: 0,
                sector_number: deal_state.sector_number,
            })),
            DealStates::V17(deal_array) => Ok(deal_array.get(key)?.map(|deal_state| DealState {
                sector_start_epoch: deal_state.sector_start_epoch,
                last_updated_epoch: deal_state.last_updated_epoch,
                slash_epoch: deal_state.slash_epoch,
                verified_claim: 0,
                sector_number: deal_state.sector_number,
            })),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct DealState {
    pub sector_start_epoch: ChainEpoch, // -1 if not yet included in proven sector
    pub last_updated_epoch: ChainEpoch, // -1 if deal state never updated
    pub slash_epoch: ChainEpoch,        // -1 if deal never slashed
    pub verified_claim: AllocationID, // ID of the verified registry allocation/claim for this deal's data (0 if none).
    pub sector_number: u64, // 0 if not yet included in proven sector (0 is also a valid sector number)
}

impl DealState {
    /// Empty deal state
    pub const fn empty() -> Self {
        Self {
            sector_start_epoch: -1,
            last_updated_epoch: -1,
            slash_epoch: -1,
            verified_claim: 0,
            sector_number: 0,
        }
    }
}

impl<BS> BalanceTable<'_, BS>
where
    BS: Blockstore,
{
    pub fn get(&self, key: &Address) -> anyhow::Result<TokenAmount> {
        Ok(delegate_balance_table!(self.get(&key.into())?.into()))
    }
}
