// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#![allow(dead_code)]

mod errors;

pub use self::errors::*;
use actor::{MinerInfo, StorageMinerActorState};
use address::Address;
use amt::BlockStore;
use chain::ChainStore;
use encoding::de::DeserializeOwned;
use state_tree::{HamtStateTree, StateTree};

/// Intermediary for retrieving state objects and updating actor states
pub struct StateManager<'a> {
    cs: &'a ChainStore<'a>,
    tree: HamtStateTree,
}

impl<'a> StateManager<'a> {
    /// constructor
    pub fn new(cs: &'a ChainStore, tree: HamtStateTree) -> Self {
        Self { cs, tree }
    }
    /// Loads actor state from IPLD Store
    fn load_actor_state<T>(&self, addr: &Address) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        let actor = self
            .tree
            .get_actor(addr)
            .ok_or_else(|| Error::State("Could not retrieve actor from state tree".to_owned()))?;
        let act: T = self.cs.blockstore().get(&actor.state)?.ok_or_else(|| {
            Error::State("Could not retrieve actor state from IPLD store".to_owned())
        })?;
        Ok(act)
    }
    /// Returns the epoch at which the miner was slashed at
    pub fn miner_slashed(&self, addr: &Address) -> Result<u64, Error> {
        let act: StorageMinerActorState = self.load_actor_state(addr)?;
        Ok(*act.slashed_at())
    }
    /// Returns the amount of space in each sector committed to the network by this miner
    pub fn miner_sector_size(&self, addr: &Address) -> Result<u64, Error> {
        let act: StorageMinerActorState = self.load_actor_state(addr)?;
        let info: MinerInfo = self.cs.blockstore().get(act.info())?.ok_or_else(|| {
            Error::State("Could not retrieve miner info from IPLD store".to_owned())
        })?;
        Ok(*info.sector_size())
    }
}
