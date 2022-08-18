// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use forest_ipld_blockstore::{BlockStore, BlockStoreExt};
use forest_json::bigint::json;
use forest_vm::TokenAmount;
use fvm::state_tree::ActorState;
use fvm_shared::address::Address;
use fvm_shared::bigint::BigInt;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::piece::PaddedPieceSize;
use serde::Serialize;
use std::marker::PhantomData;

use anyhow::Context;

/// Market actor address.
pub const ADDRESS: Address = Address::new_id(5);

/// Market actor method.
pub type Method = fil_actor_market_v8::Method;

pub fn is_v8_market_cid(cid: &Cid) -> bool {
    let known_cids = vec![
        // calibnet
        Cid::try_from("bafk2bzacebotg5coqnglzsdrqxtkqk2eq4krxt6zvds3i3vb2yejgxhexl2n6").unwrap(),
        // mainnet
        Cid::try_from("bafk2bzacediohrxkp2fbsl4yj4jlupjdkgsiwqb4zuezvinhdo2j5hrxco62q").unwrap(),
        // devnet
        Cid::try_from("bafk2bzacecw57fpkqesfhi5g3nr4csy4oy7oc42wmwjuis6l7ijniolo4rt2k").unwrap(),
    ];
    known_cids.contains(cid)
}

/// Market actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    V8(fil_actor_market_v8::State),
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> anyhow::Result<State>
    where
        BS: BlockStore,
    {
        if is_v8_market_cid(&actor.code) {
            return store
                .get_obj(&actor.state)?
                .map(State::V8)
                .context("Actor state doesn't exist in store");
        }
        Err(anyhow::anyhow!("Unknown market actor code {}", actor.code))
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
            State::V8(st) => st.total_locked(),
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
            State::V8(st) => fil_actor_market_v8::validate_deals_for_activation(
                st,
                &store,
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
    UnusedBalanceTable(PhantomData<&'a BS>),
}

pub enum DealProposals<'a, BS> {
    UnusedDealProposal(PhantomData<&'a BS>),
}

impl<BS> DealProposals<'_, BS> {
    pub fn for_each(
        &self,
        _f: impl FnMut(u64, DealProposal) -> anyhow::Result<(), anyhow::Error>,
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
    #[serde(with = "forest_json::cid", rename = "PieceCID")]
    pub piece_cid: Cid,
    pub piece_size: PaddedPieceSize,
    pub verified_deal: bool,
    #[serde(with = "forest_json::address::json")]
    pub client: Address,
    #[serde(with = "forest_json::address::json")]
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
