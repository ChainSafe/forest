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
    UnusedBalanceTable(PhantomData<&'a BS>),
}

pub enum DealProposals<'a, BS> {
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

pub enum DealStates<'a, BS> {
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

impl<BS> BalanceTable<'_, BS>
where
    BS: BlockStore,
{
    pub fn get(&self, _key: &Address) -> anyhow::Result<TokenAmount> {
        unimplemented!()
    }
}
