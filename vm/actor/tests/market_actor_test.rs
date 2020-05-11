// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod common;

use actor::{
    Multimap, SetMultimap,
    market::{State, DealState, Method},
    init::{ConstructorParams, ExecParams, ExecReturn},
    ACCOUNT_ACTOR_CODE_ID, FIRST_NON_SINGLETON_ADDR, INIT_ACTOR_CODE_ID,
    MINER_ACTOR_CODE_ID, MARKET_ACTOR_CODE_ID, MULTISIG_ACTOR_CODE_ID, PAYCH_ACTOR_CODE_ID, POWER_ACTOR_CODE_ID,
    STORAGE_POWER_ACTOR_ADDR, STORAGE_MARKET_ACTOR_ADDR, SYSTEM_ACTOR_ADDR, SYSTEM_ACTOR_CODE_ID,
};
use address::Address;
use cid::Cid;
use common::*;
use db::MemoryDB;
use ipld_blockstore::BlockStore;
use message::{Message, UnsignedMessage};
use serde::{Serialize,de::DeserializeOwned};
use vm::{ActorError, ExitCode, Serialized, TokenAmount, METHOD_CONSTRUCTOR};
use ipld_amt::Amt;


pub enum TestId{
    MarketActorId = 100,
    OwnerId  = 101,
    ProviderId  = 102,
    WorkerId  = 103,
    ClientId  = 104
}



fn setup<BS: BlockStore>(bs: &BS) -> MockRuntime<'_, BS> {

    let message = UnsignedMessage::builder()
    .to(*STORAGE_MARKET_ACTOR_ADDR)
    .from(*SYSTEM_ACTOR_ADDR)
    .build()
    .unwrap();

    let mut rt = MockRuntime::new(bs, message);

    rt.caller_type = INIT_ACTOR_CODE_ID.clone();

    rt.actor_code_cids.insert(Address::new_id(TestId::OwnerId as u64), ACCOUNT_ACTOR_CODE_ID.clone());
    rt.actor_code_cids.insert(Address::new_id(TestId::WorkerId as u64), ACCOUNT_ACTOR_CODE_ID.clone());
    rt.actor_code_cids.insert(Address::new_id(TestId::ProviderId as u64), MINER_ACTOR_CODE_ID.clone());
    rt.actor_code_cids.insert(Address::new_id(TestId::ClientId as u64), ACCOUNT_ACTOR_CODE_ID.clone());
    construct_and_verify(& mut rt);

    rt
}


// TODO add array stuff
#[test]
fn simple_construction(){
    let bs = MemoryDB::default();

    let receiver : Address  = Address::new_id(100);

    let message = UnsignedMessage::builder()
    .to(receiver.clone())
    .from(*SYSTEM_ACTOR_ADDR)
    .build()
    .unwrap();

    let mut rt = MockRuntime::new(&bs, message);
    rt.caller_type = INIT_ACTOR_CODE_ID.clone();

    rt.expect_validate_caller_addr(&[SYSTEM_ACTOR_ADDR.clone()]);

    let call_result = rt.call(
        &*MARKET_ACTOR_CODE_ID,
        METHOD_CONSTRUCTOR,
        &Serialized::default()
    ).unwrap();

    assert_eq!(call_result, Serialized::default());

    rt.verify();

    let store = rt.store;
    let empty_map = Multimap::new(store).root().unwrap();
    let empty_set = SetMultimap::new(store).root().unwrap();
    

    let state_data : State = rt.get_state().unwrap();

    assert_eq!(empty_map,state_data.escrow_table);
    assert_eq!(empty_map,state_data.locked_table);
    assert_eq!(empty_set,state_data.deal_ids_by_party);
}

#[test]
fn add_provider_escrow_funds(){

    // First element of tuple is the delta the second element is the total after the delta change
    let test_cases = vec![(10,10),(20,30),(40,70)];

    let owner_addr = Address::new_id(TestId::OwnerId as u64);
    let worker_addr = Address::new_id(TestId::WorkerId as u64);
    
    for caller_addr in vec![owner_addr, worker_addr] {
        let bs = MemoryDB::default();
        let mut rt = setup(&bs);

        for test_case in test_cases.clone() {
            rt.set_caller( ACCOUNT_ACTOR_CODE_ID.clone() , caller_addr);

            let amount = TokenAmount::from( test_case.0 as u8) ;
            rt.balance = rt.balance + amount.clone() ;
            rt.set_value(amount);
            rt.call(&ACCOUNT_ACTOR_CODE_ID.clone(), Method::AddBalance as u64, &Serialized::default());
            rt.verify();

        

        }


    }





}






fn construct_and_verify<BS: BlockStore>(rt: &mut MockRuntime<'_, BS>) {
    rt.expect_validate_caller_addr(&[SYSTEM_ACTOR_ADDR.clone()]);

    let ret = rt.call(
            &*MARKET_ACTOR_CODE_ID,
            METHOD_CONSTRUCTOR,
            &Serialized::default(),
        )
        .unwrap();

    assert_eq!(Serialized::default(), ret);
    rt.verify();
}



