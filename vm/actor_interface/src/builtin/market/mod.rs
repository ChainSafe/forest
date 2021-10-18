// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use fil_types::PaddedPieceSize;
use ipld_blockstore::BlockStore;
use num_bigint::{bigint_ser, BigInt};
use serde::Serialize;
use std::error::Error;
use vm::{ActorState, TokenAmount};

/// Market actor address.
pub static ADDRESS: &actorv4::STORAGE_MARKET_ACTOR_ADDR = &actorv4::STORAGE_MARKET_ACTOR_ADDR;

/// Market actor method.
pub type Method = actorv4::market::Method;

/// Market actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    V0(actorv0::market::State),
    V2(actorv2::market::State),
    V3(actorv3::market::State),
    V4(actorv4::market::State),
    V5(actorv5::market::State),
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> Result<State, Box<dyn Error>>
    where
        BS: BlockStore,
    {
        if actor.code == *actorv0::MARKET_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V0)
                .ok_or("Actor state doesn't exist in store")?)
        } else if actor.code == *actorv2::MARKET_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V2)
                .ok_or("Actor state doesn't exist in store")?)
        } else if actor.code == *actorv3::MARKET_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V3)
                .ok_or("Actor state doesn't exist in store")?)
        } else if actor.code == *actorv4::MARKET_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V4)
                .ok_or("Actor state doesn't exist in store")?)
        } else if actor.code == *actorv5::MARKET_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V5)
                .ok_or("Actor state doesn't exist in store")?)
        } else {
            Err(format!("Unknown actor code {}", actor.code).into())
        }
    }

    /// Loads escrow table
    pub fn escrow_table<'bs, BS>(
        &self,
        store: &'bs BS,
    ) -> Result<BalanceTable<'bs, BS>, Box<dyn Error>>
    where
        BS: BlockStore,
    {
        match self {
            State::V0(st) => {
                Ok(actorv0::BalanceTable::from_root(store, &st.escrow_table)
                    .map(BalanceTable::V0)?)
            }
            State::V2(st) => {
                Ok(actorv2::BalanceTable::from_root(store, &st.escrow_table)
                    .map(BalanceTable::V2)?)
            }
            State::V3(st) => {
                Ok(actorv3::BalanceTable::from_root(store, &st.escrow_table)
                    .map(BalanceTable::V3)?)
            }
            State::V4(st) => {
                Ok(actorv4::BalanceTable::from_root(store, &st.escrow_table)
                    .map(BalanceTable::V4)?)
            }
            State::V5(st) => {
                Ok(actorv5::BalanceTable::from_root(store, &st.escrow_table)
                    .map(BalanceTable::V5)?)
            }
        }
    }

    /// Loads locked funds table
    pub fn locked_table<'bs, BS>(
        &self,
        store: &'bs BS,
    ) -> Result<BalanceTable<'bs, BS>, Box<dyn Error>>
    where
        BS: BlockStore,
    {
        match self {
            State::V0(st) => {
                Ok(actorv0::BalanceTable::from_root(store, &st.locked_table)
                    .map(BalanceTable::V0)?)
            }
            State::V2(st) => {
                Ok(actorv2::BalanceTable::from_root(store, &st.locked_table)
                    .map(BalanceTable::V2)?)
            }
            State::V3(st) => {
                Ok(actorv3::BalanceTable::from_root(store, &st.locked_table)
                    .map(BalanceTable::V3)?)
            }
            State::V4(st) => {
                Ok(actorv4::BalanceTable::from_root(store, &st.locked_table)
                    .map(BalanceTable::V4)?)
            }
            State::V5(st) => {
                Ok(actorv5::BalanceTable::from_root(store, &st.escrow_table)
                    .map(BalanceTable::V5)?)
            }
        }
    }

    /// Deal proposals
    pub fn proposals<'bs, BS>(
        &self,
        store: &'bs BS,
    ) -> Result<DealProposals<'bs, BS>, Box<dyn Error>>
    where
        BS: BlockStore,
    {
        match self {
            State::V0(st) => Ok(
                actorv0::market::DealArray::load(&st.proposals, store).map(DealProposals::V0)?
            ),
            State::V2(st) => Ok(
                actorv2::market::DealArray::load(&st.proposals, store).map(DealProposals::V2)?
            ),
            State::V3(st) => Ok(
                actorv3::market::DealArray::load(&st.proposals, store).map(DealProposals::V3)?
            ),
            State::V4(st) => Ok(
                actorv4::market::DealArray::load(&st.proposals, store).map(DealProposals::V4)?
            ),
            State::V5(st) => Ok(
                actorv5::market::DealArray::load(&st.proposals, store).map(DealProposals::V5)?
            ),
        }
    }

    /// Deal proposal meta data.
    pub fn states<'bs, BS>(&self, store: &'bs BS) -> Result<DealStates<'bs, BS>, Box<dyn Error>>
    where
        BS: BlockStore,
    {
        match self {
            State::V0(st) => {
                Ok(actorv0::market::DealMetaArray::load(&st.states, store).map(DealStates::V0)?)
            }
            State::V2(st) => {
                Ok(actorv2::market::DealMetaArray::load(&st.states, store).map(DealStates::V2)?)
            }
            State::V3(st) => {
                Ok(actorv3::market::DealMetaArray::load(&st.states, store).map(DealStates::V3)?)
            }
            State::V4(st) => {
                Ok(actorv4::market::DealMetaArray::load(&st.states, store).map(DealStates::V4)?)
            }
            State::V5(st) => {
                Ok(actorv5::market::DealMetaArray::load(&st.states, store).map(DealStates::V5)?)
            }
        }
    }

    /// Consume state to return just total funds locked
    pub fn total_locked(&self) -> TokenAmount {
        match self {
            State::V0(st) => st.total_locked(),
            State::V2(st) => st.total_locked(),
            State::V3(st) => st.total_locked(),
            State::V4(st) => st.total_locked(),
            State::V5(st) => st.total_locked(),
        }
    }

    /// Validates a collection of deal dealProposals for activation, and returns their combined weight,
    /// split into regular deal weight and verified deal weight.
    pub fn verify_deals_for_activation<BS>(
        &self,
        store: &BS,
        deal_ids: &[u64],
        miner_addr: &Address,
        sector_expiry: ChainEpoch,
        curr_epoch: ChainEpoch,
    ) -> Result<(BigInt, BigInt), Box<dyn Error>>
    where
        BS: BlockStore,
    {
        match self {
            State::V0(st) => actorv0::market::validate_deals_for_activation(
                st,
                store,
                deal_ids,
                miner_addr,
                sector_expiry,
                curr_epoch,
            ),
            State::V2(st) => actorv2::market::validate_deals_for_activation(
                st,
                store,
                deal_ids,
                miner_addr,
                sector_expiry,
                curr_epoch,
            )
            .map(|(deal_st, verified_st, _)| (deal_st, verified_st)),
            State::V3(st) => actorv3::market::validate_deals_for_activation(
                st,
                store,
                deal_ids,
                miner_addr,
                sector_expiry,
                curr_epoch,
            )
            .map(|(deal_st, verified_st, _)| (deal_st, verified_st)),
            State::V4(st) => actorv4::market::validate_deals_for_activation(
                st,
                store,
                deal_ids,
                miner_addr,
                sector_expiry,
                curr_epoch,
            )
            .map(|(deal_st, verified_st, _)| (deal_st, verified_st)),
            State::V5(st) => actorv5::market::validate_deals_for_activation(
                st,
                store,
                deal_ids,
                miner_addr,
                sector_expiry,
                curr_epoch,
            )
            .map(|(deal_st, verified_st, _)| (deal_st, verified_st)),
        }
    }
}

pub enum BalanceTable<'a, BS> {
    V0(actorv0::BalanceTable<'a, BS>),
    V2(actorv2::BalanceTable<'a, BS>),
    V3(actorv3::BalanceTable<'a, BS>),
    V4(actorv4::BalanceTable<'a, BS>),
    V5(actorv5::BalanceTable<'a, BS>),
}

pub enum DealProposals<'a, BS> {
    V0(actorv0::market::DealArray<'a, BS>),
    V2(actorv2::market::DealArray<'a, BS>),
    V3(actorv3::market::DealArray<'a, BS>),
    V4(actorv4::market::DealArray<'a, BS>),
    V5(actorv5::market::DealArray<'a, BS>),
}

impl<BS> DealProposals<'_, BS> {
    pub fn for_each(
        &self,
        mut f: impl FnMut(u64, DealProposal) -> Result<(), Box<dyn Error>>,
    ) -> Result<(), Box<dyn Error>>
    where
        BS: BlockStore,
    {
        match self {
            DealProposals::V0(dp) => {
                dp.for_each(|idx, proposal| f(idx, DealProposal::from(proposal.clone())))
            }
            DealProposals::V2(dp) => {
                dp.for_each(|idx, proposal| f(idx, DealProposal::from(proposal.clone())))
            }
            DealProposals::V3(dp) => {
                dp.for_each(|idx, proposal| f(idx as u64, DealProposal::from(proposal.clone())))
            }
            DealProposals::V4(dp) => {
                dp.for_each(|idx, proposal| f(idx as u64, DealProposal::from(proposal.clone())))
            }
            DealProposals::V5(dp) => {
                dp.for_each(|idx, proposal| f(idx as u64, DealProposal::from(proposal.clone())))
            }
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct DealProposal {
    #[serde(with = "cid::json", rename = "PieceCID")]
    pub piece_cid: Cid,
    pub piece_size: PaddedPieceSize,
    pub verified_deal: bool,
    #[serde(with = "address::json")]
    pub client: Address,
    #[serde(with = "address::json")]
    pub provider: Address,
    // ! This is the field that requires unsafe unchecked utf8 deserialization
    pub label: String,
    pub start_epoch: ChainEpoch,
    pub end_epoch: ChainEpoch,
    #[serde(with = "bigint_ser::json")]
    pub storage_price_per_epoch: TokenAmount,
    #[serde(with = "bigint_ser::json")]
    pub provider_collateral: TokenAmount,
    #[serde(with = "bigint_ser::json")]
    pub client_collateral: TokenAmount,
}

impl From<actorv0::market::DealProposal> for DealProposal {
    fn from(d: actorv0::market::DealProposal) -> Self {
        Self {
            piece_cid: d.piece_cid,
            piece_size: d.piece_size,
            verified_deal: d.verified_deal,
            client: d.client,
            provider: d.client,
            label: d.label,
            start_epoch: d.start_epoch,
            end_epoch: d.end_epoch,
            storage_price_per_epoch: d.storage_price_per_epoch,
            provider_collateral: d.provider_collateral,
            client_collateral: d.client_collateral,
        }
    }
}

impl From<actorv2::market::DealProposal> for DealProposal {
    fn from(d: actorv2::market::DealProposal) -> Self {
        Self {
            piece_cid: d.piece_cid,
            piece_size: d.piece_size,
            verified_deal: d.verified_deal,
            client: d.client,
            provider: d.client,
            label: d.label,
            start_epoch: d.start_epoch,
            end_epoch: d.end_epoch,
            storage_price_per_epoch: d.storage_price_per_epoch,
            provider_collateral: d.provider_collateral,
            client_collateral: d.client_collateral,
        }
    }
}

impl From<actorv3::market::DealProposal> for DealProposal {
    fn from(d: actorv3::market::DealProposal) -> Self {
        Self {
            piece_cid: d.piece_cid,
            piece_size: d.piece_size,
            verified_deal: d.verified_deal,
            client: d.client,
            provider: d.client,
            label: d.label,
            start_epoch: d.start_epoch,
            end_epoch: d.end_epoch,
            storage_price_per_epoch: d.storage_price_per_epoch,
            provider_collateral: d.provider_collateral,
            client_collateral: d.client_collateral,
        }
    }
}

impl From<actorv4::market::DealProposal> for DealProposal {
    fn from(d: actorv4::market::DealProposal) -> Self {
        Self {
            piece_cid: d.piece_cid,
            piece_size: d.piece_size,
            verified_deal: d.verified_deal,
            client: d.client,
            provider: d.client,
            label: d.label,
            start_epoch: d.start_epoch,
            end_epoch: d.end_epoch,
            storage_price_per_epoch: d.storage_price_per_epoch,
            provider_collateral: d.provider_collateral,
            client_collateral: d.client_collateral,
        }
    }
}

impl From<actorv5::market::DealProposal> for DealProposal {
    fn from(d: actorv5::market::DealProposal) -> Self {
        Self {
            piece_cid: d.piece_cid,
            piece_size: d.piece_size,
            verified_deal: d.verified_deal,
            client: d.client,
            provider: d.client,
            label: d.label,
            start_epoch: d.start_epoch,
            end_epoch: d.end_epoch,
            storage_price_per_epoch: d.storage_price_per_epoch,
            provider_collateral: d.provider_collateral,
            client_collateral: d.client_collateral,
        }
    }
}

pub enum DealStates<'a, BS> {
    V0(actorv0::market::DealMetaArray<'a, BS>),
    V2(actorv2::market::DealMetaArray<'a, BS>),
    V3(actorv3::market::DealMetaArray<'a, BS>),
    V4(actorv4::market::DealMetaArray<'a, BS>),
    V5(actorv5::market::DealMetaArray<'a, BS>),
}

impl<BS> DealStates<'_, BS>
where
    BS: BlockStore,
{
    pub fn get(&self, key: u64) -> Result<Option<DealState>, Box<dyn Error>> {
        match self {
            DealStates::V0(bt) => Ok(bt.get(key)?.cloned().map(From::from)),
            DealStates::V2(bt) => Ok(bt.get(key)?.cloned().map(From::from)),
            DealStates::V3(bt) => Ok(bt.get(key as usize)?.cloned().map(From::from)),
            DealStates::V4(bt) => Ok(bt.get(key as usize)?.cloned().map(From::from)),
            DealStates::V5(bt) => Ok(bt.get(key as usize)?.cloned().map(From::from)),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct DealState {
    pub sector_start_epoch: ChainEpoch, // -1 if not yet included in proven sector
    pub last_updated_epoch: ChainEpoch, // -1 if deal state never updated
    pub slash_epoch: ChainEpoch,        // -1 if deal never slashed
}

impl From<actorv0::market::DealState> for DealState {
    fn from(d: actorv0::market::DealState) -> Self {
        Self {
            sector_start_epoch: d.sector_start_epoch,
            last_updated_epoch: d.last_updated_epoch,
            slash_epoch: d.slash_epoch,
        }
    }
}

impl From<actorv2::market::DealState> for DealState {
    fn from(d: actorv2::market::DealState) -> Self {
        Self {
            sector_start_epoch: d.sector_start_epoch,
            last_updated_epoch: d.last_updated_epoch,
            slash_epoch: d.slash_epoch,
        }
    }
}

impl From<actorv3::market::DealState> for DealState {
    fn from(d: actorv3::market::DealState) -> Self {
        Self {
            sector_start_epoch: d.sector_start_epoch,
            last_updated_epoch: d.last_updated_epoch,
            slash_epoch: d.slash_epoch,
        }
    }
}

impl From<actorv4::market::DealState> for DealState {
    fn from(d: actorv4::market::DealState) -> Self {
        Self {
            sector_start_epoch: d.sector_start_epoch,
            last_updated_epoch: d.last_updated_epoch,
            slash_epoch: d.slash_epoch,
        }
    }
}

impl From<actorv5::market::DealState> for DealState {
    fn from(d: actorv5::market::DealState) -> Self {
        Self {
            sector_start_epoch: d.sector_start_epoch,
            last_updated_epoch: d.last_updated_epoch,
            slash_epoch: d.slash_epoch,
        }
    }
}

impl<BS> BalanceTable<'_, BS>
where
    BS: BlockStore,
{
    pub fn get(&self, key: &Address) -> Result<TokenAmount, Box<dyn Error>> {
        match self {
            BalanceTable::V0(bt) => bt.get(key),
            BalanceTable::V2(bt) => bt.get(key),
            BalanceTable::V3(bt) => bt.get(key),
            BalanceTable::V4(bt) => bt.get(key),
            BalanceTable::V5(bt) => bt.get(key),
        }
    }
}
