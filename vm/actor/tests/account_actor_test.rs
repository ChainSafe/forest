use actor::{account::State, ACCOUNT_ACTOR_CODE_ID, SYSTEM_ACTOR_ADDR, SYSTEM_ACTOR_CODE_ID};
// use crate::tests::mock_rt::*;
use db::MemoryDB;
use address::Address;
use vm::Serialized;
#[path = "mock_rt.rs"]
mod mock_rt;
use mock_rt::*;
use ipld_blockstore::BlockStore;


#[test]
fn secp() {
    let bs = MemoryDB::default();

    let receiver = Address::new_id(100).unwrap();

    let mut rt = MockRuntime::new(&bs, receiver.clone());
    rt.caller = SYSTEM_ACTOR_ADDR.clone();
    rt.caller_type = SYSTEM_ACTOR_CODE_ID.clone();

    let secp_addr = Address::new_secp256k1(&[1,2,3]).unwrap();

    rt.expect_validate_caller_addr(&vec![SYSTEM_ACTOR_ADDR.clone()]);

    let _ = rt.call(&*ACCOUNT_ACTOR_CODE_ID, 1, &Serialized::serialize(secp_addr.clone()).unwrap()).unwrap();

    let x = rt.store.get_bytes(&rt.state.as_ref().unwrap()).unwrap().unwrap();

    let state: State = rt.get_state().unwrap();
    
    assert_eq!(state.address, secp_addr);
    rt.expect_validate_caller_any();
    
    let pk: Address = rt.call(&*ACCOUNT_ACTOR_CODE_ID, 2, &Serialized::default()).unwrap().deserialize().unwrap();
    assert_eq!(pk, secp_addr);
}

#[test]
fn fail () {
    let bs = MemoryDB::default();

    let receiver = Address::new_id(1).unwrap(); 
    let mut rt = MockRuntime::new(&bs, receiver.clone());
    rt.caller = SYSTEM_ACTOR_ADDR.clone();
    rt.caller_type = SYSTEM_ACTOR_CODE_ID.clone();
    rt.expect_validate_caller_addr(&vec![SYSTEM_ACTOR_ADDR.clone()]);

    let res = rt.call(&*ACCOUNT_ACTOR_CODE_ID, 1, &Serialized::serialize(Address::new_id(1).unwrap()).unwrap());

    println!("{:?}", res);
}
