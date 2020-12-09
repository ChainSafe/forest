// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use ipld_blockstore::BlockStore;
use serde::Serialize;
use std::error::Error;
use vm::{ActorState, TokenAmount};

/// Market actor address.
pub static ADDRESS: &actorv2::STORAGE_MARKET_ACTOR_ADDR = &actorv2::STORAGE_MARKET_ACTOR_ADDR;

/// Market actor method.
pub type Method = actorv2::market::Method;

/// Market actor state.
#[derive(Serialize)]
#[serde(untagged)]
pub enum State {
    V0(actorv0::market::State),
    V2(actorv2::market::State),
}

impl State {
    pub fn load<BS>(store: &BS, actor: &ActorState) -> Result<State, Box<dyn Error>>
    where
        BS: BlockStore,
    {
        if actor.code == *actorv0::SYSTEM_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V0)
                .ok_or("Actor state doesn't exist in store")?)
        } else if actor.code == *actorv2::SYSTEM_ACTOR_CODE_ID {
            Ok(store
                .get(&actor.state)?
                .map(State::V2)
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
        }
    }

    /// Consume state to return just storage power reward
    pub fn total_locked(&self) -> TokenAmount {
        match self {
            State::V0(st) => st.total_locked(),
            State::V2(st) => st.total_locked(),
        }
    }
}

pub enum BalanceTable<'a, BS> {
    V0(actorv0::BalanceTable<'a, BS>),
    V2(actorv2::BalanceTable<'a, BS>),
}

impl<BS> BalanceTable<'_, BS>
where
    BS: BlockStore,
{
    pub fn get(&self, key: &Address) -> Result<TokenAmount, Box<dyn Error>> {
        match self {
            BalanceTable::V0(bt) => bt.get(key),
            BalanceTable::V2(bt) => bt.get(key),
        }
    }
}
