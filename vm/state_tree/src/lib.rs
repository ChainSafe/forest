// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::{ActorState, INIT_ACTOR_ADDR};
use address::{Address, Protocol};
use cid::Cid;
use fnv::FnvHashMap;
use ipld_blockstore::BlockStore;
use ipld_hamt::Hamt;
use parking_lot::RwLock;

const TREE_BIT_WIDTH: u8 = 5;

pub trait StateTree {
    fn get_actor(&self, addr: &Address) -> Result<Option<ActorState>, String>;
    fn set_actor(&mut self, addr: &Address, actor: ActorState) -> Result<(), String>;
    fn lookup_id(&self, addr: &Address) -> Result<Address, String>;
    fn delete_actor(&mut self, addr: &Address) -> Result<(), String>;
    fn mutate_actor<F>(&mut self, addr: &Address, mutate: F) -> Result<(), String>
    where
        F: FnOnce(ActorState) -> Result<ActorState, String>;
    fn register_new_address(
        &mut self,
        addr: &Address,
        actor: ActorState,
    ) -> Result<Address, String>;
    fn flush(&mut self) -> Result<Cid, String>;
    fn snapshot(&mut self) -> Result<u64, String>;
    fn revert_to_snapshot(&mut self, id: u64) -> Result<(), String>;
    fn clear_snapshots(&mut self);
}

/// State tree implementation using hamt
pub struct HamtStateTree<'db, S> {
    hamt: Hamt<'db, String, ActorState, S>,

    // TODO switch cache lock from using sync mutex when usage switches to async
    actor_cache: RwLock<FnvHashMap<Address, ActorState>>,

    next_snapshot_id: u64,
    snapshots: FnvHashMap<u64, Cid>,
}

impl<'db, S> HamtStateTree<'db, S>
where
    S: BlockStore,
{
    pub fn new(store: &'db S) -> Self {
        let hamt = Hamt::new_with_bit_width(store, TREE_BIT_WIDTH);
        Self {
            hamt,
            actor_cache: RwLock::new(FnvHashMap::default()),
            next_snapshot_id: 0,
            snapshots: FnvHashMap::default(),
        }
    }

    /// Constructor for a hamt state tree given an IPLD store
    pub fn new_from_root(store: &'db S, root: &Cid) -> Result<Self, String> {
        let hamt =
            Hamt::load_with_bit_width(root, store, TREE_BIT_WIDTH).map_err(|e| e.to_string())?;
        Ok(Self {
            hamt,
            actor_cache: RwLock::new(FnvHashMap::default()),
            next_snapshot_id: 0,
            snapshots: FnvHashMap::default(),
        })
    }

    /// Index key for hamt
    pub fn hash_index(addr: &Address) -> String {
        String::from_utf8_lossy(&addr.to_bytes()).to_string()
    }
}

impl<S> StateTree for HamtStateTree<'_, S>
where
    S: BlockStore,
{
    fn get_actor(&self, addr: &Address) -> Result<Option<ActorState>, String> {
        let addr = self.lookup_id(addr)?;

        // Check cache for actor state
        if let Some(actor_state) = self.actor_cache.read().get(&addr) {
            return Ok(Some(actor_state.clone()));
        }

        // if state doesn't exist, find using hamt
        let act = self
            .hamt
            .get(&Self::hash_index(&addr))
            .map_err(|e| e.to_string())?;

        // Update cache if state was found
        if let Some(act_s) = &act {
            self.actor_cache.write().insert(addr, act_s.clone());
        }

        Ok(act)
    }

    fn set_actor(&mut self, addr: &Address, actor: ActorState) -> Result<(), String> {
        let addr = self.lookup_id(addr)?;

        // Set actor state in cache
        if let Some(act) = self.actor_cache.write().insert(addr.clone(), actor.clone()) {
            if act == actor {
                // New value is same as cached, no need to set in hamt
                return Ok(());
            }
        }

        // Set actor state in hamt
        self.hamt
            .set(Self::hash_index(&addr), actor)
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    fn lookup_id(&self, addr: &Address) -> Result<Address, String> {
        if addr.protocol() == Protocol::ID {
            return Ok(addr.clone());
        }

        // TODO address resolution
        todo!()
    }

    fn delete_actor(&mut self, addr: &Address) -> Result<(), String> {
        let addr = self.lookup_id(addr)?;

        // Remove value from cache
        self.actor_cache.write().remove(&addr);

        self.hamt
            .delete(&Self::hash_index(&addr))
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    fn mutate_actor<F>(&mut self, addr: &Address, mutate: F) -> Result<(), String>
    where
        F: FnOnce(ActorState) -> Result<ActorState, String>,
    {
        // Retrieve actor state from address
        let act: ActorState = self
            .get_actor(addr)?
            .ok_or(format!("Actor for address: {} does not exist", addr))?;

        // Apply function of actor state and set the actor
        self.set_actor(addr, mutate(act)?)
    }

    fn register_new_address(
        &mut self,
        addr: &Address,
        actor: ActorState,
    ) -> Result<Address, String> {
        let mut out_addr: Option<Address> = None;
        self.mutate_actor(&INIT_ACTOR_ADDR, |actor| {
            // TODO after updating hamt
            todo!()
        })?;

        Ok(out_addr.unwrap())
    }

    fn flush(&mut self) -> Result<Cid, String> {
        // TODO add metrics to this
        for (addr, act) in self.actor_cache.read().iter() {
            // Set each value from cache into hamt
            // TODO this shouldn't be necessary, revisit
            self.hamt
                .set(Self::hash_index(&addr), act.clone())
                .map_err(|e| e.to_string())?;
        }

        self.hamt.flush().map_err(|e| e.to_string())
    }

    fn snapshot(&mut self) -> Result<u64, String> {
        let cid = self.flush()?;

        let id = self.next_snapshot_id;
        self.next_snapshot_id += 1;

        self.snapshots.insert(id, cid);

        Ok(id)
    }

    fn clear_snapshots(&mut self) {
        self.next_snapshot_id = 0;
        self.snapshots = Default::default();
    }

    fn revert_to_snapshot(&mut self, id: u64) -> Result<(), String> {
        let cid = self
            .snapshots
            .remove(&id)
            .ok_or(format!("Invalid snapshot id: {}", id))?;

        // Update Hamt root to snapshot Cid
        self.hamt.set_root(&cid).map_err(|e| e.to_string())?;

        self.actor_cache = Default::default();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actor::ActorState;
    use cid::Cid;
    use num_bigint::BigUint;

    #[test]
    fn get_set_cache() {
        let act_s = ActorState::new(Cid::default(), Cid::default(), BigUint::default(), 1);
        let act_a = ActorState::new(Cid::default(), Cid::default(), BigUint::default(), 2);
        let addr = Address::new_id(1).unwrap();
        let store = db::MemoryDB::default();
        let mut tree = HamtStateTree::new(&store);

        // test address not in cache
        assert_eq!(tree.get_actor(&addr).unwrap(), None);
        // test successful insert
        assert_eq!(tree.set_actor(&addr, act_s.clone()), Ok(()));
        // test inserting with different data
        assert_eq!(tree.set_actor(&addr, act_a.clone()), Ok(()));
        // Assert insert with same data returns ok
        assert_eq!(tree.set_actor(&addr, act_a.clone()), Ok(()));
        // test getting set item
        assert_eq!(tree.get_actor(&addr).unwrap().unwrap(), act_a);
    }
}
