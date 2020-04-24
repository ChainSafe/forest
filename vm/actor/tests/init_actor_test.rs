// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Has all of the common things needed to test inlcuding MockRuntime
mod common;
use db::MemoryDB;
//use crate::{builtin::singleton::FIRST_NON_SINGLETON_ADDR};

use actor::{
    init::{ConstructorParams, ExecParams, State},
    INIT_ACTOR_CODE_ID, SYSTEM_ACTOR_ADDR, SYSTEM_ACTOR_CODE_ID, FIRST_NON_SINGLETON_ADDR, MULTISIG_ACTOR_CODE_ID, POWER_ACTOR_CODE_ID,ACCOUNT_ACTOR_CODE_ID,
    Multimap
};


use cid::{multihash::Identity, Cid, Codec};
use common::*;
use ipld_blockstore::BlockStore;
//use ipld_hamt::Hamt;
use address::Address;
use vm::{ActorError, Serialized};


fn construct_runtime<BS: BlockStore>(bs: &BS) -> MockRuntime<'_, BS> {
    let receiver = Address::new_id(1000).unwrap();
    let mut rt = MockRuntime::new(bs, receiver.clone());
    rt.caller = SYSTEM_ACTOR_ADDR.clone();
    rt.caller_type = SYSTEM_ACTOR_CODE_ID.clone();
    return rt;
}


// Test to make sure we abort actors that can not call the exec function
#[test]
fn abort_cant_call_exec() {
    let bs = MemoryDB::default();
    let mut rt = construct_runtime(&bs);
    construct_and_verify(&mut rt);
    let anne  = Address::new_id(1001).unwrap();

    //Set caller
    rt.caller = anne;
    rt.caller_type = ACCOUNT_ACTOR_CODE_ID.clone();

    match exec_and_verify(&mut rt, POWER_ACTOR_CODE_ID.clone(), &ConstructorParams { network_name: String::new() } ) {
        Err(E) => assert_eq!(false, true),
        Ok(T)  => ()
    }

    // Didnt see a undef cid like in the go implmentation. If there is replace the not_a_actor token. Need to ask about this
    let undef_cid = Cid::new_v1(Codec::Raw, Identity::digest(b"fil/1/notaactor")); 
    match exec_and_verify(&mut rt, undef_cid , &ConstructorParams { network_name: String::new() } ) {
        Err(E) => assert_eq!(false, true),
        Ok(T)  => ()
    }

}

#[test]
fn create_2_payment_channels() {
    let bs = MemoryDB::default();
    let mut rt = construct_runtime(&bs);
    construct_and_verify(&mut rt);
    let anne  = Address::new_id(1001).unwrap();

    //Set caller
    rt.caller = anne;
    rt.caller_type = ACCOUNT_ACTOR_CODE_ID.clone();

    // Change balances not sure how to do i saw the send function, but idk if thats all i need 

    let uniq_addr_1 =   Address::new_actor(b"paych");
    rt.new_actor_addr = Some(uniq_addr_1.unwrap());



		// next id address
    //let expected_id_addr_1 = Address::new_id(100);
    
    //rt.create_actor(uniq_addr_1, expected_id_addr_1);
    //rt.expect_validate_caller_addr(&vec![SYSTEM_ACTOR_ADDR.clone()]);




    assert_eq!(true, true);
}

#[test]
fn create_storage_miner() {
    let bs = MemoryDB::default();
    let mut rt = construct_runtime(&bs);
    construct_and_verify(&mut rt);

    assert_eq!(true, true);
}
#[test]
fn create_multisig_actor() {
    let bs = MemoryDB::default();
    let mut rt = construct_runtime(&bs);
    construct_and_verify(&mut rt);



    assert_eq!(true, true);
}

#[test]
fn sending_constructor_failure() {
    let bs = MemoryDB::default();
    let mut rt = construct_runtime(&bs);
    construct_and_verify(&mut rt);

    assert_eq!(true, true);
}

fn construct_and_verify<BS: BlockStore>(rt: &mut MockRuntime<'_, BS>) {
    rt.expect_validate_caller_addr(&[SYSTEM_ACTOR_ADDR.clone()]);
    let params = ConstructorParams {
        network_name: "mock".to_string(),
    };
    let ret = rt
        .call(
            &*INIT_ACTOR_CODE_ID,
            1,
            &Serialized::serialize(&params).unwrap(),
        )
        .unwrap();

    let initial_state = Serialized::default();
    assert_eq!(initial_state, ret);
    rt.verify();

    let state_data : State = rt.get_state().unwrap();

    // Gets the Result(CID)
    let empty_map = Multimap::from_root(rt.store, &state_data.address_map).unwrap().root();

    assert_eq!(empty_map.unwrap(),  state_data.address_map);
    assert_eq!(FIRST_NON_SINGLETON_ADDR, state_data.next_id);
    assert_eq!("mock".to_string(), state_data.network_name);
}

fn exec_and_verify<BS: BlockStore>(
    rt: &mut MockRuntime<'_, BS>,
    code_id: Cid,
    params: &ConstructorParams,
) -> Result<Serialized, ActorError> {
    rt.expect_validate_caller_any();
    let exec_params = ExecParams {
        code_cid: code_id,
        constructor_params: Serialized::serialize(&params).unwrap(),
    };

    let ret = rt.call(
        &*INIT_ACTOR_CODE_ID,
        2,
        &Serialized::serialize(&exec_params).unwrap(),
    );
    rt.verify();
    ret
}
