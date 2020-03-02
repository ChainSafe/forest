// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod errors;

pub use self::errors::*;
use actor::{MinerInfo, StorageMinerActorState};
use address::Address;
use blockstore::BlockStore;
use encoding::de::DeserializeOwned;
use state_tree::StateTree;

/// Intermediary for retrieving state objects and updating actor states
pub struct StateManager<'db, DB, ST> {
    bs: &'db DB,
    tree: ST,
}

impl<'db, DB, ST> StateManager<'db, DB, ST>
where
    ST: StateTree,
    DB: BlockStore,
{
    /// constructor
    pub fn new(bs: &'db DB, tree: ST) -> Self {
        Self { bs, tree }
    }
    /// Loads actor state from IPLD Store
    fn load_actor_state<D>(&self, addr: &Address) -> Result<D, Error>
    where
        D: DeserializeOwned,
    {
        let actor = self
            .tree
            .get_actor(addr)
            .ok_or_else(|| Error::State("Could not retrieve actor from state tree".to_owned()))?;
        let act: D = self.bs.get(&actor.state)?.ok_or_else(|| {
            Error::State("Could not retrieve actor state from IPLD store".to_owned())
        })?;
        Ok(act)
    }
    /// Returns the epoch at which the miner was slashed at
    pub fn miner_slashed(&self, addr: &Address) -> Result<u64, Error> {
        let act: StorageMinerActorState = self.load_actor_state(addr)?;
        Ok(act.slashed_at)
    }
    /// Returns the amount of space in each sector committed to the network by this miner
    pub fn miner_sector_size(&self, addr: &Address) -> Result<u64, Error> {
        let act: StorageMinerActorState = self.load_actor_state(addr)?;
        let info: MinerInfo = self.bs.get(&act.info)?.ok_or_else(|| {
            Error::State("Could not retrieve miner info from IPLD store".to_owned())
        })?;
        Ok(*info.sector_size())
    }
}
