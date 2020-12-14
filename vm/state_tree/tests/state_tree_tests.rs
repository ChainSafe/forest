// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::actorv0::{
    init, ActorState, ACCOUNT_ACTOR_CODE_ID, INIT_ACTOR_ADDR, INIT_ACTOR_CODE_ID,
};
use address::{Address, SECP_PUB_LEN};
use cid::{
    Cid,
    Code::{Blake2b256, Identity},
};
use fil_types::StateTreeVersion;
use ipld_blockstore::BlockStore;
use ipld_hamt::Hamt;
use state_tree::*;

fn empty_cid() -> Cid {
    cid::new_from_cbor(&[], Identity)
}

#[test]
fn get_set_cache() {
    let act_s = ActorState::new(empty_cid(), empty_cid(), Default::default(), 1);
    let act_a = ActorState::new(empty_cid(), empty_cid(), Default::default(), 2);
    let addr = Address::new_id(1);
    let store = db::MemoryDB::default();
    let mut tree = StateTree::new(&store, StateTreeVersion::V0).unwrap();

    // test address not in cache
    assert_eq!(tree.get_actor(&addr).unwrap(), None);
    // test successful insert
    assert!(tree.set_actor(&addr, act_s.clone()).is_ok());
    // test inserting with different data
    assert!(tree.set_actor(&addr, act_a.clone()).is_ok());
    // Assert insert with same data returns ok
    assert!(tree.set_actor(&addr, act_a.clone()).is_ok());
    // test getting set item
    assert_eq!(tree.get_actor(&addr).unwrap().unwrap(), act_a);
}

#[test]
fn delete_actor() {
    let store = db::MemoryDB::default();
    let mut tree = StateTree::new(&store, StateTreeVersion::V0).unwrap();

    let addr = Address::new_id(3);
    let act_s = ActorState::new(empty_cid(), empty_cid(), Default::default(), 1);
    tree.set_actor(&addr, act_s.clone()).unwrap();
    assert_eq!(tree.get_actor(&addr).unwrap(), Some(act_s));
    tree.delete_actor(&addr).unwrap();
    assert_eq!(tree.get_actor(&addr).unwrap(), None);
}

#[test]
fn get_set_non_id() {
    let store = db::MemoryDB::default();
    let mut tree = StateTree::new(&store, StateTreeVersion::V0).unwrap();

    // Empty hamt Cid used for testing
    let e_cid = Hamt::<_, String>::new_with_bit_width(&store, 5)
        .flush()
        .unwrap();

    let init_state = init::State::new(e_cid.clone(), "test".to_owned());
    let state_cid = tree
        .store()
        .put(&init_state, Blake2b256)
        .map_err(|e| e.to_string())
        .unwrap();

    let act_s = ActorState::new(
        *INIT_ACTOR_CODE_ID,
        state_cid.clone(),
        Default::default(),
        1,
    );

    tree.snapshot().unwrap();
    tree.set_actor(&INIT_ACTOR_ADDR, act_s.clone()).unwrap();

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
            code: *INIT_ACTOR_CODE_ID,
            state: state_cid,
            balance: Default::default(),
            sequence: 2
        })
    );

    // Register new address
    let addr = Address::new_secp256k1(&[2; SECP_PUB_LEN]).unwrap();
    let assigned_addr = tree.register_new_address(&addr).unwrap();

    assert_eq!(assigned_addr, Address::new_id(100));
}

#[test]
fn test_snapshots() {
    let store = db::MemoryDB::default();
    let mut tree = StateTree::new(&store, StateTreeVersion::V0).unwrap();
    let mut addresses: Vec<Address> = Vec::new();
    use num_bigint::BigInt;

    let test_addresses = vec!["t0100", "t0101", "t0102"];
    for a in test_addresses.iter() {
        addresses.push(a.parse().unwrap());
    }

    tree.snapshot().unwrap();
    tree.set_actor(
        &addresses[0],
        ActorState::new(
            ACCOUNT_ACTOR_CODE_ID.clone(),
            ACCOUNT_ACTOR_CODE_ID.clone(),
            BigInt::from(55),
            1,
        ),
    )
    .unwrap();

    tree.set_actor(
        &addresses[1],
        ActorState::new(
            ACCOUNT_ACTOR_CODE_ID.clone(),
            ACCOUNT_ACTOR_CODE_ID.clone(),
            BigInt::from(55),
            1,
        ),
    )
    .unwrap();
    tree.set_actor(
        &addresses[2],
        ActorState::new(
            ACCOUNT_ACTOR_CODE_ID.clone(),
            ACCOUNT_ACTOR_CODE_ID.clone(),
            BigInt::from(55),
            1,
        ),
    )
    .unwrap();
    tree.clear_snapshot().unwrap();
    tree.flush().unwrap();

    assert_eq!(
        tree.get_actor(&addresses[0]).unwrap().unwrap(),
        ActorState::new(
            ACCOUNT_ACTOR_CODE_ID.clone(),
            ACCOUNT_ACTOR_CODE_ID.clone(),
            BigInt::from(55),
            1
        )
    );
    assert_eq!(
        tree.get_actor(&addresses[1]).unwrap().unwrap(),
        ActorState::new(
            ACCOUNT_ACTOR_CODE_ID.clone(),
            ACCOUNT_ACTOR_CODE_ID.clone(),
            BigInt::from(55),
            1
        )
    );

    assert_eq!(
        tree.get_actor(&addresses[2]).unwrap().unwrap(),
        ActorState::new(
            ACCOUNT_ACTOR_CODE_ID.clone(),
            ACCOUNT_ACTOR_CODE_ID.clone(),
            BigInt::from(55),
            1
        )
    );
}

#[test]
fn revert_snapshot() {
    let store = db::MemoryDB::default();
    let mut tree = StateTree::new(&store, StateTreeVersion::V0).unwrap();
    use num_bigint::BigInt;

    let addr_str = "f01";
    let addr: Address = addr_str.parse().unwrap();

    tree.snapshot().unwrap();
    tree.set_actor(
        &addr,
        ActorState::new(
            ACCOUNT_ACTOR_CODE_ID.clone(),
            ACCOUNT_ACTOR_CODE_ID.clone(),
            BigInt::from(55),
            1,
        ),
    )
    .unwrap();
    tree.revert_to_snapshot().unwrap();
    tree.clear_snapshot().unwrap();

    tree.flush().unwrap();

    assert_eq!(tree.get_actor(&addr).unwrap(), None);
}
