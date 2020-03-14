// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::ActorState;
use address::Address;
use cid::Cid;

/// Interface to allow for the retreival and modification of Actors and their state
pub trait StateTree {
    /// Get actor state from an address. Will be resolved to ID address.
    fn get_actor(&self, addr: &Address) -> Result<Option<ActorState>, String>;
    /// Set actor state for an address. Will set state at ID address.
    fn set_actor(&mut self, addr: &Address, actor: ActorState) -> Result<(), String>;
    /// Get an ID address from any Address
    fn lookup_id(&self, addr: &Address) -> Result<Address, String>;
    /// Delete actor for an address. Will resolve to ID address to delete.
    fn delete_actor(&mut self, addr: &Address) -> Result<(), String>;
    /// Mutate and set actor state for an Address.
    fn mutate_actor<F>(&mut self, addr: &Address, mutate: F) -> Result<(), String>
    where
        F: FnOnce(ActorState) -> Result<ActorState, String>;
    /// Register a new address through the init actor.
    fn register_new_address(
        &mut self,
        addr: &Address,
        actor: ActorState,
    ) -> Result<Address, String>;
    /// Persist changes to store and return Cid to revert state to.
    fn snapshot(&mut self) -> Result<Cid, String>;
    /// Revert to Cid returned from `snapshot`
    fn revert_to_snapshot(&mut self, cid: &Cid) -> Result<(), String>;
}
