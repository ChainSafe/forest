use actor::ActorState;
use address::Address;
use std::collections::HashMap;

pub trait StateTree {
    fn get_actor(&self, addr: Address) -> Option<ActorState>;
    fn set_actor(&mut self, addr: Address, actor: ActorState) -> Result<(), String>;
}

struct HamtNode; // TODO
struct Store; // TODO
/// State tree implementation using hamt
pub struct HamtStateTree {
    /// hamt root node
    _root: HamtNode,
    /// IPLD store
    _store: Store,

    actor_cache: HashMap<Address, ActorState>,
}

impl Default for HamtStateTree {
    /// Constructor for a hamt state tree given an IPLD store
    fn default() -> Self {
        Self {
            _root: HamtNode,
            _store: Store,
            actor_cache: HashMap::new(),
        }
    }
}

impl StateTree for HamtStateTree {
    fn get_actor(&self, address: Address) -> Option<ActorState> {
        // TODO resolve ID address

        // Check cache for actor state
        if let Some(addr) = self.actor_cache.get(&address) {
            return Some(addr.clone());
        }

        // if state doesn't exist, find using hamt
        // TODO
        None
    }
    fn set_actor(&mut self, addr: Address, actor: ActorState) -> Result<(), String> {
        // TODO resolve ID address

        // Set actor state in cache
        if let Some(act) = self.actor_cache.insert(addr, actor.clone()) {
            if act == actor {
                // New value is same as cached, no need to set in hamt
                return Ok(());
            }
        }

        // Set actor state in hamt
        // TODO
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actor::{ActorState, CodeID};
    use cid::{Cid, Codec, Version};
    use num_bigint::BigUint;

    #[test]
    fn get_set_cache() {
        let cid = Cid::new(Codec::DagProtobuf, Version::V1, &[0u8]);
        let act_s = ActorState::new(CodeID::Account, cid.clone(), BigUint::default(), 1);
        let act_a = ActorState::new(CodeID::Account, cid.clone(), BigUint::default(), 2);
        let addr = Address::new_id(1).unwrap();
        let mut tree = HamtStateTree::default();

        // test address not in cache
        assert_eq!(tree.get_actor(addr.clone()), None);
        // test successful insert
        assert_eq!(tree.set_actor(addr.clone(), act_s.clone()), Ok(()));
        // test inserting with different data
        assert_eq!(tree.set_actor(addr.clone(), act_a.clone()), Ok(()));
        // Assert insert with same data returns ok
        assert_eq!(tree.set_actor(addr.clone(), act_a.clone()), Ok(()));
        // test getting set item
        assert_eq!(tree.get_actor(addr.clone()).unwrap(), act_a);
    }
}
