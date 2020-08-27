// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::{init, INIT_ACTOR_ADDR};
use address::{Address, Protocol};
use cid::{multihash::Blake2b256, Cid};
use fil_types::HAMT_BIT_WIDTH;
use fnv::FnvHashMap;
use ipld_blockstore::BlockStore;
use ipld_hamt::Hamt;
use parking_lot::RwLock;
use std::error::Error as StdError;
use vm::ActorState;

/// State tree implementation using hamt
pub struct StateTree<'db, S> {
    hamt: Hamt<'db, S>,

    /// State cache
    snaps: StateSnapshots,
}

/// Collection of state snapshots
struct StateSnapshots {
    layers: Vec<Option<StateSnapLayer>>,
}

/// State snap shot layer
struct StateSnapLayer {
    actors: RwLock<FnvHashMap<Address, Option<ActorState>>>,
    resolve_cache: RwLock<FnvHashMap<Address, Address>>,
}

impl StateSnapLayer {
    /// Snapshot layer constructor
    fn new() -> Self {
        Self {
            actors: RwLock::new(FnvHashMap::default()),
            resolve_cache: RwLock::new(FnvHashMap::default()),
        }
    }
}

impl StateSnapshots {
    /// State snapshot constructor
    fn new() -> Self {
        Self {
            layers: vec![Some(StateSnapLayer::new())],
        }
    }

    fn add_layer(&mut self) {
        self.layers.push(Some(StateSnapLayer::new()))
    }

    fn drop_layer(&mut self) {
        let idx = &self.layers.len() - 1;
        self.layers[idx] = None;
        //self.layers = self.layers[..self.layers.len() - 1].to_vec();
    }

    fn merge_last_layer(&mut self) -> Result<(), Box<dyn StdError>> {
        let idx = &self.layers.len() - 1;

        let last: StateSnapLayer = self.layers[idx].ok_or_else(|| {
            format!(
                "No snapshot layer found at index {}",
                &self.layers.len() - 1
            )
            .to_owned()
        })?;

        let idx_2 = &self.layers.len() - 2;
        let next_last: StateSnapLayer = self.layers[idx_2].ok_or_else(|| {
            format!(
                "No snapshot layer found at index {}",
                &self.layers.len() - 2
            )
            .to_owned()
        })?;

        for (&k, &v) in last.actors.read().iter() {
            next_last.actors.write().insert(k, v);
        }

        for (&k, &v) in last.resolve_cache.read().iter() {
            next_last.resolve_cache.write().insert(k, v);
        }

        self.drop_layer();
        Ok(())
    }

    fn resolve_address(&self, addr: &Address) -> Result<Option<Address>, Box<dyn StdError>> {
        let mut i = self.layers.len() - 1;
        while i >= 0 {
            if let Some(layer) = &self.layers[i] {
                let resolve_addr = layer.resolve_cache.read().get(addr).ok_or_else(|| {
                    format!("No resolve address found at address {}", addr).to_owned()
                })?;
                return Ok(Some(*resolve_addr));
            }
            i -= 1;
        }
        Ok(None)
    }

    fn cache_resolve_address(&self, addr: Address, resolve_addr: Address) {
        if let Some(layer) = &self.layers[self.layers.len() - 1] {
            layer.resolve_cache.write().insert(addr, resolve_addr);
        } else {
            println!("Failed to cache resolve addresses");
        }
    }

    fn get_actor(&self, addr: &Address) -> Result<Option<ActorState>, Box<dyn StdError>> {
        let mut i = self.layers.len() - 1;
        while i >= 0 {
            let layer: &StateSnapLayer = self.layers[i]
                .as_ref()
                .ok_or_else(|| format!("No snapshot layer found at index {}", i))?;
            let actor_state = layer.actors.read().get(addr).ok_or_else(|| {
                format!("No cached actor state found at address {}", addr).to_owned()
            })?;
            i -= 1;
            return Ok(*actor_state);
        }
        Ok(None)
    }

    fn set_actor(&self, addr: Address, actor: ActorState) -> Result<(), Box<dyn StdError>> {
        let layer = self.layers[&self.layers.len() - 1]
            .as_ref()
            .ok_or_else(|| {
                format!(
                    "No snapshot layer found at index: {}",
                    &self.layers.len() - 1
                )
            })?;
        layer.actors.write().insert(addr, Some(actor));
        Ok(())
    }

    fn delete_actor(&self, addr: Address) -> Result<(), Box<dyn StdError>> {
        let layer = self.layers[&self.layers.len() - 1]
            .as_ref()
            .ok_or_else(|| {
                format!(
                    "No snapshot layer found at index: {}",
                    &self.layers.len() - 1
                )
            })?;
        layer.actors.write().insert(addr, None);

        Ok(())
    }
}

impl<'db, S> StateTree<'db, S>
where
    S: BlockStore,
{
    pub fn new(store: &'db S) -> Self {
        let hamt = Hamt::new_with_bit_width(store, HAMT_BIT_WIDTH);
        Self {
            hamt,
            snaps: StateSnapshots::new(),
        }
    }

    /// Constructor for a hamt state tree given an IPLD store
    pub fn new_from_root(store: &'db S, root: &Cid) -> Result<Self, Box<dyn StdError>> {
        let hamt =
            Hamt::load_with_bit_width(root, store, HAMT_BIT_WIDTH).map_err(|e| e.to_string())?;
        Ok(Self {
            hamt,
            snaps: StateSnapshots::new(),
        })
    }

    /// Retrieve store reference to modify db.
    pub fn store(&self) -> &S {
        self.hamt.store()
    }

    /// Get actor state from an address. Will be resolved to ID address.
    pub fn get_actor(&self, addr: &Address) -> Result<Option<ActorState>, Box<dyn StdError>> {
        let addr = self
            .lookup_id(addr)?
            .ok_or_else(|| format!("Resolution lookup failed for {}", addr))?;

        // Check cache for actor state
        if let Some(actor_state) = self.snaps.get_actor(&addr)? {
            return Ok(Some(actor_state.clone()));
        }

        // if state doesn't exist, find using hamt
        let act: Option<ActorState> = self.hamt.get(&addr.to_bytes()).map_err(|e| e.to_string())?;

        // Update cache if state was found
        if let Some(act_s) = &act {
            self.snaps.set_actor(addr, act_s.clone());
        }

        Ok(act)
    }

    /// Set actor state for an address. Will set state at ID address.
    pub fn set_actor(
        &mut self,
        addr: &Address,
        actor: ActorState,
    ) -> Result<(), Box<dyn StdError>> {
        let addr = self
            .lookup_id(addr)?
            .ok_or_else(|| format!("Resolution lookup failed for {}", addr).to_owned())?;

        // Set actor state in cache
        self.snaps.set_actor(addr, actor.clone());

        // Set actor state in hamt
        self.hamt
            .set(addr.to_bytes().into(), actor)
            .map_err(|e| format!("Err setting HAMT {}", e).to_owned())?;

        Ok(())
    }

    /// Get an ID address from any Address
    pub fn lookup_id(&mut self, addr: &Address) -> Result<Option<Address>, Box<dyn StdError>> {
        if addr.protocol() == Protocol::ID {
            return Ok(Some(*addr));
        }

        match self.snaps.resolve_address(addr)? {
            None => println!("No address cached"),
            Some(resa) => return Ok(Some(resa)),
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

        let a: Address = state
            .resolve_address(self.store(), addr)
            .map_err(|e| format!("Could not resolve address: {:?}", e))?
            .ok_or_else(|| format!("Resolve address failed for {}", addr))?;

        self.snaps.cache_resolve_address(*addr, a);

        Ok(Some(a))
    }

    /// Delete actor for an address. Will resolve to ID address to delete.
    pub fn delete_actor(&mut self, addr: &Address) -> Result<(), Box<dyn StdError>> {
        let addr = self
            .lookup_id(addr)?
            .ok_or_else(|| format!("Resolution lookup failed for {}", addr))?;

        // Remove value from cache
        self.snaps.delete_actor(addr);

        self.hamt
            .delete(&addr.to_bytes())
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Mutate and set actor state for an Address.
    pub fn mutate_actor<F>(&mut self, addr: &Address, mutate: F) -> Result<(), Box<dyn StdError>>
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
    pub fn register_new_address(&mut self, addr: &Address) -> Result<Address, Box<dyn StdError>> {
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

    /// Persist changes to store and return Cid to revert state to.
    pub fn snapshot(&mut self) {
        self.snaps.add_layer();
    }

    /// Clears state snapshot cache
    pub fn clear_snapshot(&mut self) {
        self.snaps.merge_last_layer();
    }

    /// Revert to Cid returned from `snapshot`
    pub fn revert_to_snapshot(&mut self) -> Result<(), Box<dyn StdError>> {
        self.snaps.drop_layer();
        self.snaps.add_layer();
        Ok(())
    }

    /// Flush state tree and return Cid root.
    pub fn flush(&mut self) -> Result<Cid, Box<dyn StdError>> {
        if self.snaps.layers.len() != 1 {
            return Err(format!("Tried to flush state tree with snapshots on the stack").into());
        }

        let layers = self.snaps.layers[0]
            .as_ref()
            .ok_or_else(|| format!("No snapshot layer at index {}", 0))?;
        for (addr, sto) in layers.actors.read().iter() {
            match sto {
                None => {
                    self.hamt
                        .delete(&addr.to_bytes())
                        .map_err(|e| e.to_string())?;
                }
                Some(state) => {
                    self.hamt
                        .set(addr.to_bytes().into(), &state)
                        .map_err(|e| e.to_string())?;
                }
            }
        }

        Ok(self.hamt.flush().map_err(|e| e.to_string())?)
    }
}
