// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod errors;

pub use self::errors::*;
use actor::{miner, power, ActorState, STORAGE_POWER_ACTOR_ADDR};
use address::Address;
use blockstore::BlockStore;
use cid::Cid;
use default_runtime::resolve_to_key_addr;
use encoding::de::DeserializeOwned;
use num_bigint::BigUint;
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
    fn load_actor_state<D>(&self, addr: &Address, state_cid: &Cid) -> Result<D, Error>
    where
        D: DeserializeOwned,
    {
        let actor = self
            .get_actor(addr, state_cid)?
            .ok_or_else(|| Error::ActorNotFound(addr.to_string()))?;
        let act: D = self
            .bs
            .get(&actor.state)
            .map_err(|e| Error::State(e.to_string()))?
            .ok_or_else(|| Error::ActorStateNotFound(actor.state.to_string()))?;
        Ok(act)
    }
    fn get_actor(&self, addr: &Address, state_cid: &Cid) -> Result<Option<ActorState>, Error> {
        let state =
            HamtStateTree::new_from_root(self.bs.as_ref(), state_cid).map_err(Error::State)?;
        state.get_actor(addr).map_err(Error::State)
    }
    /// Returns true if miner has been slashed or is considered invalid
    pub fn is_miner_slashed(&self, addr: &Address, state_cid: &Cid) -> Result<bool, Error> {
        let ms: miner::State = self.load_actor_state(addr, state_cid)?;
        if ms.post_state.has_failed_post() {
            return Ok(true);
        }

        let ps: power::State = self.load_actor_state(&*STORAGE_POWER_ACTOR_ADDR, state_cid)?;
        match ps.get_claim(self.bs.as_ref(), addr)? {
            Some(_) => Ok(false),
            None => Ok(true),
        }
    }
    /// Returns raw work address of a miner
    pub fn get_miner_work_addr(&self, state_cid: &Cid, addr: &Address) -> Result<Address, Error> {
        let ms: miner::State = self.load_actor_state(addr, state_cid)?;

        let state =
            HamtStateTree::new_from_root(self.bs.as_ref(), state_cid).map_err(Error::State)?;
        // Note: miner::State info likely to be changed to CID
        let addr = resolve_to_key_addr(&state, self.bs.as_ref(), &ms.info.worker)
            .map_err(|e| Error::Other(format!("Failed to resolve key address; error: {}", e)))?;
        Ok(addr)
    }
    /// Returns specified actor's claimed power and total network power as a tuple
    pub fn get_power(&self, state_cid: &Cid, addr: &Address) -> Result<(BigUint, BigUint), Error> {
        let ps: power::State = self.load_actor_state(&*STORAGE_POWER_ACTOR_ADDR, state_cid)?;

        if let Some(claim) = ps.get_claim(self.bs.as_ref(), addr)? {
            Ok((claim.power, ps.total_network_power))
        } else {
            Err(Error::State(
                "Failed to retrieve claimed power from actor state".to_owned(),
            ))
        }
    }
}
