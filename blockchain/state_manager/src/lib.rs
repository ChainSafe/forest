// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod errors;

pub use self::errors::*;
use actor::{miner, ActorState};
use address::Address;
use blockstore::BlockStore;
use encoding::de::DeserializeOwned;
use forest_blocks::Tipset;
use state_tree::{HamtStateTree, StateTree};
use std::sync::Arc;

/// Intermediary for retrieving state objects and updating actor states
pub struct StateManager<DB> {
    bs: Arc<DB>,
}

impl<DB> StateManager<DB>
where
    DB: BlockStore,
{
    /// constructor
    pub fn new(bs: Arc<DB>) -> Self {
        Self { bs }
    }
    /// Loads actor state from IPLD Store
    fn load_actor_state<D>(&self, addr: &Address, ts: &Tipset) -> Result<D, Error>
    where
        D: DeserializeOwned,
    {
        let actor = self
            .get_actor(addr, ts)?
            .ok_or_else(|| Error::State(format!("Actor for address: {} does not exist", addr)))?;
        let act: D = self.bs.get(&actor.state)?.ok_or_else(|| {
            Error::State("Could not retrieve actor state from IPLD store".to_owned())
        })?;
        Ok(act)
    }
    /// Returns the epoch at which the miner was slashed at
    pub fn miner_slashed(&self, addr: &Address, ts: &Tipset) -> Result<u64, Error> {
        let act: miner::State = self.load_actor_state(addr, ts)?;
        Ok(act.slashed_at)
    }
    /// Returns the amount of space in each sector committed to the network by this miner
    pub fn miner_sector_size(&self, addr: &Address, ts: &Tipset) -> Result<u64, Error> {
        let act: miner::State = self.load_actor_state(addr, ts)?;
        let info: miner::MinerInfo = self.bs.get(&act.info)?.ok_or_else(|| {
            Error::State("Could not retrieve miner info from IPLD store".to_owned())
        })?;
        Ok(*info.sector_size())
    }
    pub fn get_actor(&self, addr: &Address, ts: &Tipset) -> Result<Option<ActorState>, Error> {
        let state = HamtStateTree::new_from_root(self.bs.as_ref(), ts.parent_state())
            .map_err(Error::State)?;
        state.get_actor(addr).map_err(Error::State)
    }
}
