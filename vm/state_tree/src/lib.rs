// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::{init, ActorVersion, Map};
use address::{Address, Protocol};
use cid::{Cid, Code::Blake2b256};
use fil_types::{StateInfo0, StateRoot, StateTreeVersion};
use ipld_blockstore::BlockStore;
use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error as StdError;
use vm::ActorState;

/// State tree implementation using hamt. This structure is not threadsafe and should only be used
/// in sync contexts.
pub struct StateTree<'db, S> {
    hamt: Map<'db, S, ActorState>,

    version: StateTreeVersion,
    info: Option<Cid>,

    /// State cache
    snaps: StateSnapshots,
}

/// Collection of state snapshots
struct StateSnapshots {
    layers: Vec<StateSnapLayer>,
}

/// State snap shot layer
#[derive(Debug, Default)]
struct StateSnapLayer {
    actors: RefCell<HashMap<Address, Option<ActorState>>>,
    resolve_cache: RefCell<HashMap<Address, Address>>,
}

impl StateSnapshots {
    /// State snapshot constructor
    fn new() -> Self {
        Self {
            layers: vec![StateSnapLayer::default()],
        }
    }

    fn add_layer(&mut self) {
        self.layers.push(StateSnapLayer::default())
    }

    fn drop_layer(&mut self) -> Result<(), String> {
        self.layers.pop().ok_or_else(|| {
            format!(
                "drop layer failed to index snapshot layer at index {}",
                &self.layers.len() - 1
            )
        })?;

        Ok(())
    }

    fn merge_last_layer(&mut self) -> Result<(), String> {
        self.layers
            .get(&self.layers.len() - 2)
            .ok_or_else(|| {
                format!(
                    "merging layers failed to index snapshot layer at index: {}",
                    &self.layers.len() - 2
                )
            })?
            .actors
            .borrow_mut()
            .extend(
                self.layers[&self.layers.len() - 1]
                    .actors
                    .borrow_mut()
                    .drain(),
            );

        self.layers
            .get(&self.layers.len() - 2)
            .ok_or_else(|| {
                format!(
                    "merging layers failed to index snapshot layer at index: {}",
                    &self.layers.len() - 2
                )
            })?
            .resolve_cache
            .borrow_mut()
            .extend(
                self.layers[&self.layers.len() - 1]
                    .resolve_cache
                    .borrow_mut()
                    .drain(),
            );

        self.drop_layer()
    }

    fn resolve_address(&self, addr: &Address) -> Option<Address> {
        for layer in self.layers.iter().rev() {
            if let Some(res_addr) = layer.resolve_cache.borrow().get(addr).cloned() {
                return Some(res_addr);
            }
        }

        None
    }

    fn cache_resolve_address(
        &self,
        addr: Address,
        resolve_addr: Address,
    ) -> Result<(), Box<dyn StdError>> {
        self.layers
            .last()
            .ok_or_else(|| {
                format!(
                    "caching address failed to index snapshot layer at index: {}",
                    &self.layers.len() - 1
                )
            })?
            .resolve_cache
            .borrow_mut()
            .insert(addr, resolve_addr);

        Ok(())
    }

    fn get_actor(&self, addr: &Address) -> Option<ActorState> {
        for layer in self.layers.iter().rev() {
            if let Some(state) = layer.actors.borrow().get(addr) {
                return state.clone();
            }
        }

        None
    }

    fn set_actor(&self, addr: Address, actor: ActorState) -> Result<(), Box<dyn StdError>> {
        self.layers
            .last()
            .ok_or_else(|| {
                format!(
                    "set actor failed to index snapshot layer at index: {}",
                    &self.layers.len() - 1
                )
            })?
            .actors
            .borrow_mut()
            .insert(addr, Some(actor));
        Ok(())
    }

    fn delete_actor(&self, addr: Address) -> Result<(), Box<dyn StdError>> {
        self.layers
            .last()
            .ok_or_else(|| {
                format!(
                    "delete actor failed to index snapshot layer at index: {}",
                    &self.layers.len() - 1
                )
            })?
            .actors
            .borrow_mut()
            .insert(addr, None);

        Ok(())
    }
}

impl<'db, S> StateTree<'db, S>
where
    S: BlockStore,
{
    pub fn new(store: &'db S, version: StateTreeVersion) -> Result<Self, Box<dyn StdError>> {
        let info = match version {
            StateTreeVersion::V0 => None,
            StateTreeVersion::V1 | StateTreeVersion::V2 => {
                Some(store.put(&StateInfo0::default(), Blake2b256)?)
            }
        };

        let hamt = Map::new(store, ActorVersion::from(version));
        Ok(Self {
            hamt,
            version,
            info,
            snaps: StateSnapshots::new(),
        })
    }

    /// Constructor for a hamt state tree given an IPLD store
    pub fn new_from_root(store: &'db S, c: &Cid) -> Result<Self, Box<dyn StdError>> {
        // Try to load state root, if versioned
        let (version, info, actors) = if let Ok(Some(StateRoot {
            version,
            info,
            actors,
        })) = store.get(c)
        {
            (version, Some(info), actors)
        } else {
            // Fallback to v0 state tree if retrieval fails
            (StateTreeVersion::V0, None, *c)
        };

        match version {
            StateTreeVersion::V0 | StateTreeVersion::V1 | StateTreeVersion::V2 => {
                let hamt = Map::load(&actors, store, version.into())?;

                Ok(Self {
                    hamt,
                    version,
                    info,
                    snaps: StateSnapshots::new(),
                })
            }
        }
    }

    /// Retrieve store reference to modify db.
    pub fn store(&self) -> &S {
        self.hamt.store()
    }

    /// Get actor state from an address. Will be resolved to ID address.
    pub fn get_actor(&self, addr: &Address) -> Result<Option<ActorState>, Box<dyn StdError>> {
        let addr = match self.lookup_id(addr)? {
            Some(addr) => addr,
            None => return Ok(None),
        };

        // Check cache for actor state
        if let Some(actor_state) = self.snaps.get_actor(&addr) {
            return Ok(Some(actor_state));
        }

        // if state doesn't exist, find using hamt
        let act = self.hamt.get(&addr.to_bytes())?.cloned();

        // Update cache if state was found
        if let Some(act_s) = &act {
            self.snaps.set_actor(addr, act_s.clone())?;
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
            .ok_or_else(|| format!("Resolution lookup failed for {}", addr))?;

        self.snaps.set_actor(addr, actor)
    }

    /// Get an ID address from any Address
    pub fn lookup_id(&self, addr: &Address) -> Result<Option<Address>, Box<dyn StdError>> {
        if addr.protocol() == Protocol::ID {
            return Ok(Some(*addr));
        }

        if let Some(res_address) = self.snaps.resolve_address(addr) {
            return Ok(Some(res_address));
        }

        let init_act = self
            .get_actor(actor::init::ADDRESS)?
            .ok_or("Init actor address could not be resolved")?;

        let state = init::State::load(self.hamt.store(), &init_act)?;

        let a: Address = match state
            .resolve_address(self.store(), addr)
            .map_err(|e| format!("Could not resolve address: {:?}", e))?
        {
            Some(a) => a,
            None => return Ok(None),
        };

        self.snaps.cache_resolve_address(*addr, a)?;

        Ok(Some(a))
    }

    /// Delete actor for an address. Will resolve to ID address to delete.
    pub fn delete_actor(&mut self, addr: &Address) -> Result<(), Box<dyn StdError>> {
        let addr = self
            .lookup_id(addr)?
            .ok_or_else(|| format!("Resolution lookup failed for {}", addr))?;

        // Remove value from cache
        self.snaps.delete_actor(addr)?;

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
        let mut actor: ActorState = self
            .get_actor(init::ADDRESS)?
            .ok_or("Could not retrieve init actor")?;

        let mut ias = init::State::load(self.store(), &actor)?;

        let new_addr = ias.map_address_to_new_id(self.store(), addr)?;

        // Set state for init actor in store and update root Cid
        actor.state = self.store().put(&ias, Blake2b256)?;

        self.set_actor(init::ADDRESS, actor)?;

        Ok(new_addr)
    }

    /// Add snapshot layer to stack.
    pub fn snapshot(&mut self) -> Result<(), String> {
        self.snaps.add_layer();
        Ok(())
    }

    /// Merges last two snap shot layers.
    pub fn clear_snapshot(&mut self) -> Result<(), String> {
        self.snaps.merge_last_layer()
    }

    /// Revert state cache by removing last snapshot
    pub fn revert_to_snapshot(&mut self) -> Result<(), String> {
        self.snaps.drop_layer()?;
        self.snaps.add_layer();
        Ok(())
    }

    /// Flush state tree and return Cid root.
    pub fn flush(&mut self) -> Result<Cid, Box<dyn StdError>> {
        if self.snaps.layers.len() != 1 {
            return Err(format!(
                "tried to flush state tree with snapshots on the stack: {:?}",
                self.snaps.layers.len()
            )
            .into());
        }

        for (addr, sto) in self.snaps.layers[0].actors.borrow().iter() {
            match sto {
                None => {
                    self.hamt.delete(&addr.to_bytes())?;
                }
                Some(ref state) => {
                    self.hamt.set(addr.to_bytes().into(), state.clone())?;
                }
            }
        }

        let root = self.hamt.flush()?;

        if matches!(self.version, StateTreeVersion::V0) {
            Ok(root)
        } else {
            Ok(self.store().put(
                &StateRoot {
                    version: self.version,
                    actors: root,
                    info: self
                        .info
                        .expect("malformed state tree, version 1 and version 2 require info"),
                },
                Blake2b256,
            )?)
        }
    }

    pub fn for_each<F>(&self, mut f: F) -> Result<(), Box<dyn StdError>>
    where
        F: FnMut(Address, &ActorState) -> Result<(), Box<dyn StdError>>,
        S: BlockStore,
    {
        self.hamt.for_each(|k, v| f(Address::from_bytes(&k.0)?, v))
    }
}
