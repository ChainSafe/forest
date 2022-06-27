// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use cid::multihash::MultihashDigest;
use cid::Cid;
use clock::ChainEpoch;
use fil_types::PaddedPieceSize;
use fvm_shared::bigint::BigInt;
use ipld_blockstore::BlockStore;
use num_bigint::bigint_ser::json;
use serde::Serialize;
use std::{error::Error, marker::PhantomData};
use vm::{ActorState, TokenAmount};

use anyhow::Context;

/// Market actor address.
pub static ADDRESS: &fil_actors_runtime_v7::builtin::singletons::STORAGE_MARKET_ACTOR_ADDR =
    &fil_actors_runtime_v7::builtin::singletons::STORAGE_MARKET_ACTOR_ADDR;

/// Market actor method.
pub type Method = fil_actor_market_v7::Method;

/// Market actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    // V0(actorv0::market::State),
    // V2(actorv2::market::State),
    // V3(actorv3::market::State),
    // V4(actorv4::market::State),
    // V5(actorv5::market::State),
    // V6(actorv6::market::State),
    V7(fil_actor_market_v7::State),
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> anyhow::Result<State>
    where
        BS: BlockStore,
    {
        if actor.code
            == cid::Cid::new_v1(cid::RAW, cid::Code::Identity.digest(b"fil/7/storagemarket"))
        {
            Ok(store
                .get_anyhow(&actor.state)?
                .map(State::V7)
                .context("Actor state doesn't exist in store")?)
        } else {
            Err(anyhow::anyhow!("Unknown market actor code {}", actor.code))
        }
    }

    /// Loads escrow table
    pub fn escrow_table<'bs, BS>(&self, _store: &'bs BS) -> anyhow::Result<BalanceTable<'bs, BS>>
    where
        BS: BlockStore,
    {
        unimplemented!()
    }

    /// Loads locked funds table
    pub fn locked_table<'bs, BS>(&self, _store: &'bs BS) -> anyhow::Result<BalanceTable<'bs, BS>>
    where
        BS: BlockStore,
    {
        unimplemented!()
    }

    /// Deal proposals
    pub fn proposals<'bs, BS>(&self, _store: &'bs BS) -> anyhow::Result<DealProposals<'bs, BS>>
    where
        BS: BlockStore,
    {
        unimplemented!()
    }

    /// Deal proposal meta data.
    pub fn states<'bs, BS>(&self, _store: &'bs BS) -> anyhow::Result<DealStates<'bs, BS>>
    where
        BS: BlockStore,
    {
        unimplemented!()
    }

    /// Consume state to return just total funds locked
    pub fn total_locked(&self) -> TokenAmount {
        match self {
            // State::V0(st) => st.total_locked(),
            // State::V2(st) => st.total_locked(),
            // State::V3(st) => st.total_locked(),
            // State::V4(st) => st.total_locked(),
            // State::V5(st) => st.total_locked(),
            // State::V6(st) => st.total_locked(),
            State::V7(st) => st.total_locked(),
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
    ) -> anyhow::Result<(BigInt, BigInt)>
    where
        BS: BlockStore,
    {
        match self {
            // State::V0(st) => actorv0::market::validate_deals_for_activation(
            //     st,
            //     store,
            //     deal_ids,
            //     miner_addr,
            //     sector_expiry,
            //     curr_epoch,
            // ),
            // State::V2(st) => actorv2::market::validate_deals_for_activation(
            //     st,
            //     store,
            //     deal_ids,
            //     miner_addr,
            //     sector_expiry,
            //     curr_epoch,
            // )
            // .map(|(deal_st, verified_st, _)| (deal_st, verified_st)),
            // State::V3(st) => actorv3::market::validate_deals_for_activation(
            //     st,
            //     store,
            //     deal_ids,
            //     miner_addr,
            //     sector_expiry,
            //     curr_epoch,
            // )
            // .map(|(deal_st, verified_st, _)| (deal_st, verified_st)),
            // State::V4(st) => actorv4::market::validate_deals_for_activation(
            //     st,
            //     store,
            //     deal_ids,
            //     miner_addr,
            //     sector_expiry,
            //     curr_epoch,
            // )
            // .map(|(deal_st, verified_st, _)| (deal_st, verified_st)),
            // State::V5(st) => actorv5::market::validate_deals_for_activation(
            //     st,
            //     store,
            //     deal_ids,
            //     miner_addr,
            //     sector_expiry,
            //     curr_epoch,
            // )
            // .map(|(deal_st, verified_st, _)| (deal_st, verified_st)),
            // State::V6(st) => actorv6::market::validate_deals_for_activation(
            //     st,
            //     store,
            //     deal_ids,
            //     miner_addr,
            //     sector_expiry,
            //     curr_epoch,
            // )
            // .map(|(deal_st, verified_st, _)| (deal_st, verified_st)),
            State::V7(st) => {
                let fvm_store = ipld_blockstore::FvmRefStore::new(store);
                Ok(fil_actor_market_v7::validate_deals_for_activation(
                    st,
                    &fvm_store,
                    deal_ids,
                    miner_addr,
                    sector_expiry,
                    curr_epoch,
                )
                .map(|(deal_st, verified_st, _)| (deal_st, verified_st))
                .expect("FIXME"))
            } // _ => unimplemented!(),
        }
    }
}

pub enum BalanceTable<'a, BS> {
    // V0(actorv0::BalanceTable<'a, BS>),
    // V2(actorv2::BalanceTable<'a, BS>),
    // V3(actorv3::BalanceTable<'a, BS>),
    // V4(actorv4::BalanceTable<'a, BS>),
    // V5(actorv5::BalanceTable<'a, BS>),
    // V6(actorv6::BalanceTable<'a, BS>),
    UnusedBalanceTable(PhantomData<&'a BS>),
}

pub enum DealProposals<'a, BS> {
    // V0(actorv0::market::DealArray<'a, BS>),
    // V2(actorv2::market::DealArray<'a, BS>),
    // V3(actorv3::market::DealArray<'a, BS>),
    // V4(actorv4::market::DealArray<'a, BS>),
    // V5(actorv5::market::DealArray<'a, BS>),
    // V6(actorv6::market::DealArray<'a, BS>),
    UnusedDealProposal(PhantomData<&'a BS>),
}

impl<BS> DealProposals<'_, BS> {
    pub fn for_each(
        &self,
        _f: impl FnMut(u64, DealProposal) -> anyhow::Result<(), Box<dyn Error>>,
    ) -> anyhow::Result<()>
    where
        BS: BlockStore,
    {
        unimplemented!()
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
    #[serde(with = "json")]
    pub storage_price_per_epoch: TokenAmount,
    #[serde(with = "json")]
    pub provider_collateral: TokenAmount,
    #[serde(with = "json")]
    pub client_collateral: TokenAmount,
}

// impl From<actorv0::market::DealProposal> for DealProposal {
//     fn from(d: actorv0::market::DealProposal) -> Self {
//         Self {
//             piece_cid: d.piece_cid,
//             piece_size: d.piece_size,
//             verified_deal: d.verified_deal,
//             client: d.client,
//             provider: d.client,
//             label: d.label,
//             start_epoch: d.start_epoch,
//             end_epoch: d.end_epoch,
//             storage_price_per_epoch: d.storage_price_per_epoch,
//             provider_collateral: d.provider_collateral,
//             client_collateral: d.client_collateral,
//         }
//     }
// }

// impl From<actorv2::market::DealProposal> for DealProposal {
//     fn from(d: actorv2::market::DealProposal) -> Self {
//         Self {
//             piece_cid: d.piece_cid,
//             piece_size: d.piece_size,
//             verified_deal: d.verified_deal,
//             client: d.client,
//             provider: d.client,
//             label: d.label,
//             start_epoch: d.start_epoch,
//             end_epoch: d.end_epoch,
//             storage_price_per_epoch: d.storage_price_per_epoch,
//             provider_collateral: d.provider_collateral,
//             client_collateral: d.client_collateral,
//         }
//     }
// }

// impl From<actorv3::market::DealProposal> for DealProposal {
//     fn from(d: actorv3::market::DealProposal) -> Self {
//         Self {
//             piece_cid: d.piece_cid,
//             piece_size: d.piece_size,
//             verified_deal: d.verified_deal,
//             client: d.client,
//             provider: d.client,
//             label: d.label,
//             start_epoch: d.start_epoch,
//             end_epoch: d.end_epoch,
//             storage_price_per_epoch: d.storage_price_per_epoch,
//             provider_collateral: d.provider_collateral,
//             client_collateral: d.client_collateral,
//         }
//     }
// }

// impl From<actorv4::market::DealProposal> for DealProposal {
//     fn from(d: actorv4::market::DealProposal) -> Self {
//         Self {
//             piece_cid: d.piece_cid,
//             piece_size: d.piece_size,
//             verified_deal: d.verified_deal,
//             client: d.client,
//             provider: d.client,
//             label: d.label,
//             start_epoch: d.start_epoch,
//             end_epoch: d.end_epoch,
//             storage_price_per_epoch: d.storage_price_per_epoch,
//             provider_collateral: d.provider_collateral,
//             client_collateral: d.client_collateral,
//         }
//     }
// }

// impl From<actorv5::market::DealProposal> for DealProposal {
//     fn from(d: actorv5::market::DealProposal) -> Self {
//         Self {
//             piece_cid: d.piece_cid,
//             piece_size: d.piece_size,
//             verified_deal: d.verified_deal,
//             client: d.client,
//             provider: d.client,
//             label: d.label,
//             start_epoch: d.start_epoch,
//             end_epoch: d.end_epoch,
//             storage_price_per_epoch: d.storage_price_per_epoch,
//             provider_collateral: d.provider_collateral,
//             client_collateral: d.client_collateral,
//         }
//     }
// }

// impl From<actorv6::market::DealProposal> for DealProposal {
//     fn from(d: actorv6::market::DealProposal) -> Self {
//         Self {
//             piece_cid: d.piece_cid,
//             piece_size: d.piece_size,
//             verified_deal: d.verified_deal,
//             client: d.client,
//             provider: d.client,
//             label: d.label,
//             start_epoch: d.start_epoch,
//             end_epoch: d.end_epoch,
//             storage_price_per_epoch: d.storage_price_per_epoch,
//             provider_collateral: d.provider_collateral,
//             client_collateral: d.client_collateral,
//         }
//     }
// }

pub enum DealStates<'a, BS> {
    // V0(actorv0::market::DealMetaArray<'a, BS>),
    // V2(actorv2::market::DealMetaArray<'a, BS>),
    // V3(actorv3::market::DealMetaArray<'a, BS>),
    // V4(actorv4::market::DealMetaArray<'a, BS>),
    // V5(actorv5::market::DealMetaArray<'a, BS>),
    // V6(actorv6::market::DealMetaArray<'a, BS>),
    DealStates(PhantomData<&'a BS>),
}

impl<BS> DealStates<'_, BS>
where
    BS: BlockStore,
{
    pub fn get(&self, _key: u64) -> anyhow::Result<Option<DealState>> {
        unimplemented!()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct DealState {
    pub sector_start_epoch: ChainEpoch, // -1 if not yet included in proven sector
    pub last_updated_epoch: ChainEpoch, // -1 if deal state never updated
    pub slash_epoch: ChainEpoch,        // -1 if deal never slashed
}

// impl From<actorv0::market::DealState> for DealState {
//     fn from(d: actorv0::market::DealState) -> Self {
//         Self {
//             sector_start_epoch: d.sector_start_epoch,
//             last_updated_epoch: d.last_updated_epoch,
//             slash_epoch: d.slash_epoch,
//         }
//     }
// }

// impl From<actorv2::market::DealState> for DealState {
//     fn from(d: actorv2::market::DealState) -> Self {
//         Self {
//             sector_start_epoch: d.sector_start_epoch,
//             last_updated_epoch: d.last_updated_epoch,
//             slash_epoch: d.slash_epoch,
//         }
//     }
// }

// impl From<actorv3::market::DealState> for DealState {
//     fn from(d: actorv3::market::DealState) -> Self {
//         Self {
//             sector_start_epoch: d.sector_start_epoch,
//             last_updated_epoch: d.last_updated_epoch,
//             slash_epoch: d.slash_epoch,
//         }
//     }
// }

// impl From<actorv4::market::DealState> for DealState {
//     fn from(d: actorv4::market::DealState) -> Self {
//         Self {
//             sector_start_epoch: d.sector_start_epoch,
//             last_updated_epoch: d.last_updated_epoch,
//             slash_epoch: d.slash_epoch,
//         }
//     }
// }

// impl From<actorv5::market::DealState> for DealState {
//     fn from(d: actorv5::market::DealState) -> Self {
//         Self {
//             sector_start_epoch: d.sector_start_epoch,
//             last_updated_epoch: d.last_updated_epoch,
//             slash_epoch: d.slash_epoch,
//         }
//     }
// }

// impl From<actorv6::market::DealState> for DealState {
//     fn from(d: actorv6::market::DealState) -> Self {
//         Self {
//             sector_start_epoch: d.sector_start_epoch,
//             last_updated_epoch: d.last_updated_epoch,
//             slash_epoch: d.slash_epoch,
//         }
//     }
// }

impl<BS> BalanceTable<'_, BS>
where
    BS: BlockStore,
{
    pub fn get(&self, _key: &Address) -> anyhow::Result<TokenAmount> {
        unimplemented!()
    }
}
