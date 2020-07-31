// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::{init, INIT_ACTOR_ADDR};
use address::{Address, Protocol};
use cid::{multihash::Blake2b256, Cid};
use fnv::FnvHashMap;
use ipld_blockstore::BlockStore;
use ipld_hamt::{BytesKey, Hamt};
use parking_lot::RwLock;
use vm::ActorState;

const TREE_BIT_WIDTH: u8 = 5;

/// State tree implementation using hamt
pub struct StateTree<'db, S> {
    hamt: Hamt<'db, BytesKey, S>,

    // TODO switch to using state change cache: https://github.com/ChainSafe/forest/issues/373
    actor_cache: RwLock<FnvHashMap<Address, ActorState>>,
}

impl<'db, S> StateTree<'db, S>
where
    S: BlockStore,
{
    pub fn new(store: &'db S) -> Self {
        let hamt = Hamt::new_with_bit_width(store, TREE_BIT_WIDTH);
        Self {
            hamt,
            actor_cache: RwLock::new(FnvHashMap::default()),
        }
    }

    /// Constructor for a hamt state tree given an IPLD store
    pub fn new_from_root(store: &'db S, root: &Cid) -> Result<Self, String> {
        let hamt =
            Hamt::load_with_bit_width(root, store, TREE_BIT_WIDTH).map_err(|e| e.to_string())?;
        Ok(Self {
            hamt,
            actor_cache: RwLock::new(FnvHashMap::default()),
        })
    }

    /// Retrieve store reference to modify db.
    pub fn store(&self) -> &S {
        self.hamt.store()
    }

    /// Get actor state from an address. Will be resolved to ID address.
    pub fn get_actor(&self, addr: &Address) -> Result<Option<ActorState>, String> {
        let addr = self
            .lookup_id(addr)?
            .ok_or_else(|| format!("Resolution lookup failed for {}", addr))?;

        // Check cache for actor state
        if let Some(actor_state) = self.actor_cache.read().get(&addr) {
            return Ok(Some(actor_state.clone()));
        }

        // if state doesn't exist, find using hamt
        let act: Option<ActorState> = self.hamt.get(&addr.to_bytes()).map_err(|e| e.to_string())?;

        // Update cache if state was found
        if let Some(act_s) = &act {
            self.actor_cache.write().insert(addr, act_s.clone());
        }

        Ok(act)
    }

    /// Set actor state for an address. Will set state at ID address.
    pub fn set_actor(&mut self, addr: &Address, actor: ActorState) -> Result<(), String> {
        let addr = self
            .lookup_id(addr)?
            .ok_or_else(|| format!("Resolution lookup failed for {}", addr))?;

        // Set actor state in cache
        if let Some(act) = self.actor_cache.write().insert(addr, actor.clone()) {
            if act == actor {
                // New value is same as cached, no need to set in hamt
                return Ok(());
            }
        }

        // Set actor state in hamt
        self.hamt
            .set(addr.to_bytes().into(), actor)
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Get an ID address from any Address
    pub fn lookup_id(&self, addr: &Address) -> Result<Option<Address>, String> {
        if addr.protocol() == Protocol::ID {
            return Ok(Some(*addr));
        }

        let init_act = self
            .get_actor(&INIT_ACTOR_ADDR)?
            .ok_or("Init actor address could not be resolved")?;

        let state: init::State = self
            .hamt
            .store()
            .get(&init_act.state)
            .map_err(|e| e.to_string())?
            .ok_or("Could not resolve init actor state")?;

        state.resolve_address(self.store(), addr)
    }

    /// Delete actor for an address. Will resolve to ID address to delete.
    pub fn delete_actor(&mut self, addr: &Address) -> Result<(), String> {
        let addr = self
            .lookup_id(addr)?
            .ok_or_else(|| format!("Resolution lookup failed for {}", addr))?;

        // Remove value from cache
        self.actor_cache.write().remove(&addr);

        self.hamt
            .delete(&addr.to_bytes())
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Mutate and set actor state for an Address.
    pub fn mutate_actor<F>(&mut self, addr: &Address, mutate: F) -> Result<(), String>
    where
        F: FnOnce(&mut ActorState) -> Result<(), String>,
    {
        // Retrieve actor state from address
        let mut act: ActorState = self
            .get_actor(addr)?
            .ok_or(format!("Actor for address: {} does not exist", addr))?;

        // Apply function of actor state
        mutate(&mut act)?;
        // Set the actor
        self.set_actor(addr, act)
    }

    /// Register a new address through the init actor.
    pub fn register_new_address(&mut self, addr: &Address) -> Result<Address, String> {
        let mut init_act: ActorState = self
            .get_actor(&INIT_ACTOR_ADDR)?
            .ok_or("Could not retrieve init actor")?;

        // Get init actor state from store
        let mut ias: init::State = self
            .hamt
            .store()
            .get(&init_act.state)
            .map_err(|e| e.to_string())?
            .ok_or("Failed to retrieve init actor state")?;

        // Create new address with init actor state
        let new_addr = ias
            .map_address_to_new_id(self.store(), addr)
            .map_err(|e| e.to_string())?;

        // Set state for init actor in store and update root Cid
        init_act.state = self
            .store()
            .put(&ias, Blake2b256)
            .map_err(|e| e.to_string())?;

        self.set_actor(&INIT_ACTOR_ADDR, init_act)?;

        Ok(new_addr)
    }

    // TODO update snapshotting to not flush tree: https://github.com/ChainSafe/forest/issues/373
    /// Persist changes to store and return Cid to revert state to.
    pub fn snapshot(&mut self) -> Result<Cid, String> {
        self.flush()
    }

    /// Revert to Cid returned from `snapshot`
    pub fn revert_to_snapshot(&mut self, cid: &Cid) -> Result<(), String> {
        // Update Hamt root to snapshot Cid
        self.hamt.set_root(cid).map_err(|e| e.to_string())?;

        self.actor_cache = Default::default();
        Ok(())
    }

    /// Flush state tree and return Cid root.
    pub fn flush(&mut self) -> Result<Cid, String> {
        for (addr, act) in self.actor_cache.read().iter() {
            // Set each value from cache into hamt
            self.hamt
                .set(addr.to_bytes().into(), act.clone())
                .map_err(|e| e.to_string())?;
        }

        self.hamt.flush().map_err(|e| e.to_string())
    }
}
