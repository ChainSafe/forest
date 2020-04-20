// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::{init, ActorState, INIT_ACTOR_ADDR};
use address::Address;
use cid::{multihash::Identity, Cid};
use ipld_blockstore::BlockStore;
use ipld_hamt::Hamt;
use state_tree::*;

fn empty_cid() -> Cid {
    Cid::new_from_cbor(&[], Identity)
}

#[test]
fn get_set_cache() {
    let act_s = ActorState::new(empty_cid(), empty_cid(), Default::default(), 1);
    let act_a = ActorState::new(empty_cid(), empty_cid(), Default::default(), 2);
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
    let act_s = ActorState::new(empty_cid(), empty_cid(), Default::default(), 1);
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

    let init_state = init::State::new(e_cid.clone(), "test".to_owned());
    let state_cid = tree
        .store()
        .put(&init_state, Identity)
        .map_err(|e| e.to_string())
        .unwrap();

    let act_s = ActorState::new(empty_cid(), state_cid.clone(), Default::default(), 1);

    // Test snapshot
    let snapshot = tree.snapshot().unwrap();
    tree.set_actor(&INIT_ACTOR_ADDR, act_s.clone()).unwrap();
    assert_ne!(&tree.snapshot().unwrap(), &snapshot);

    // Test mutate function
    tree.mutate_actor(&INIT_ACTOR_ADDR, |mut actor| {
        actor.sequence = 2;
        Ok(())
    })
    .unwrap();
    let new_init_s = tree.get_actor(&INIT_ACTOR_ADDR).unwrap();
    assert_eq!(
        new_init_s,
        Some(ActorState {
            code: empty_cid(),
            state: state_cid,
            balance: Default::default(),
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
    assert_eq!(tree.get_actor(&addr).unwrap(), Some(secp_state));

    // Test reverting snapshot to before init actor set
    tree.revert_to_snapshot(&snapshot).unwrap();
    assert_eq!(tree.snapshot().unwrap(), snapshot);
    assert_eq!(tree.get_actor(&INIT_ACTOR_ADDR).unwrap(), None);
}
