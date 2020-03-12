// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::{ActorState, InitActorState, INIT_ACTOR_ADDR};
use address::{Address, Protocol};
use cid::{multihash::Blake2b256, Cid};
use fnv::FnvHashMap;
use ipld_blockstore::BlockStore;
use ipld_hamt::Hamt;
use parking_lot::RwLock;

const TREE_BIT_WIDTH: u8 = 5;

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

/// State tree implementation using hamt
pub struct HamtStateTree<'db, S> {
    hamt: Hamt<'db, String, S>,

    // TODO switch cache lock from using sync mutex when usage switches to async
    actor_cache: RwLock<FnvHashMap<Address, ActorState>>,
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

    /// Index key for hamt
    pub fn hash_index(addr: &Address) -> String {
        String::from_utf8_lossy(&addr.to_bytes()).to_string()
    }

    /// Retrieve store reference to modify db.
    fn store(&self) -> &S {
        self.hamt.store()
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
        let act: Option<ActorState> = self
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

        let init_act = self
            .get_actor(&INIT_ACTOR_ADDR)?
            .ok_or("Init actor address could not be resolved".to_owned())?;

        let state: InitActorState = self
            .hamt
            .store()
            .get(&init_act.state)
            .map_err(|e| e.to_string())?
            .ok_or("Could not resolve init actor state")?;

        Ok(state.resolve_address(self.store(), addr)?)
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
        let mut init_act: ActorState = self
            .get_actor(&INIT_ACTOR_ADDR)?
            .ok_or("Could not retrieve init actor".to_owned())?;

        // Get init actor state from store
        let mut ias: InitActorState = self
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

        self.set_actor(&new_addr, actor)?;

        Ok(new_addr)
    }

    fn snapshot(&mut self) -> Result<Cid, String> {
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

    fn revert_to_snapshot(&mut self, cid: &Cid) -> Result<(), String> {
        // Update Hamt root to snapshot Cid
        self.hamt.set_root(cid).map_err(|e| e.to_string())?;

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

    #[test]
    fn delete_actor() {
        let store = db::MemoryDB::default();
        let mut tree = HamtStateTree::new(&store);

        let addr = Address::new_id(3).unwrap();
        let act_s = ActorState::new(Cid::default(), Cid::default(), BigUint::default(), 1);
        tree.set_actor(&addr, act_s.clone()).unwrap();
        assert_eq!(tree.get_actor(&addr).unwrap(), Some(act_s));
        tree.delete_actor(&addr).unwrap();
        assert_eq!(tree.get_actor(&addr).unwrap(), None);
    }

    #[test]
    fn get_set_non_id() {
        let store = db::MemoryDB::default();
        let mut tree = HamtStateTree::new(&store);

        // Empty hamt Cid used for testing
        let e_cid = Hamt::<String, _>::new_with_bit_width(&store, 5)
            .flush()
            .unwrap();

        let init_state = InitActorState::new(e_cid.clone());
        let state_cid = tree
            .store()
            .put(&init_state, Blake2b256)
            .map_err(|e| e.to_string())
            .unwrap();

        let act_s = ActorState::new(Cid::default(), state_cid.clone(), BigUint::default(), 1);

        // Test snapshot
        let snapshot = tree.snapshot().unwrap();
        tree.set_actor(&INIT_ACTOR_ADDR, act_s.clone()).unwrap();
        assert_ne!(&tree.snapshot().unwrap(), &snapshot);

        // Test mutate function
        tree.mutate_actor(&INIT_ACTOR_ADDR, |mut actor| {
            actor.sequence = 2;
            Ok(actor)
        })
        .unwrap();
        let new_init_s = tree.get_actor(&INIT_ACTOR_ADDR).unwrap();
        assert_eq!(
            new_init_s,
            Some(ActorState {
                code: Cid::default(),
                state: state_cid,
                balance: BigUint::default(),
                sequence: 2
            })
        );

        // Register new address
        let addr = Address::new_secp256k1(&[0, 2]).unwrap();
        let secp_state = ActorState::new(e_cid.clone(), e_cid.clone(), Default::default(), 0);
        let assigned_addr = tree
            .register_new_address(&addr, secp_state.clone())
            .unwrap();

        assert_eq!(assigned_addr, Address::new_id(100).unwrap());

        // Test resolution of Secp address
        // TODO enable this test when the init actor address resolution completed
        // assert_eq!(tree.get_actor(&addr).unwrap(), Some(secp_state));

        // Test reverting snapshot to before init actor set
        tree.revert_to_snapshot(&snapshot).unwrap();
        assert_eq!(tree.snapshot().unwrap(), snapshot);
        assert_eq!(tree.get_actor(&INIT_ACTOR_ADDR).unwrap(), None);
    }
}
