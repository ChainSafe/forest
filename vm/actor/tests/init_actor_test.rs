// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Has all of the common things needed to test inlcuding MockRuntime
mod common;
use common::*;
use cid::{multihash::Blake2b256, Cid};
use ipld_blockstore::BlockStore;
use actor::{
    SYSTEM_ACTOR_ADDR,
    INIT_ACTOR_CODE_ID,
    init::{ConstructorParams,ExecParams} ,
};

use vm::{ActorError,Serialized};

// Test to make sure we abort actors that can not call the exec function
#[test]
fn abort_cant_call_exec() {
    assert_eq!(true, true);
}

#[test]
fn create_2_payment_channels() {
    assert_eq!(true, true);
}

#[test]
fn create_storage_miner() {
    assert_eq!(true, true);
}
#[test]
fn create_multisig_actor() {

    //let bs = MemoryDB::default();

    // rt := builder.Build(t)
    // actor.constructAndVerify(rt)
    // // actor creating the multisig actor
    // someAccountActor := tutil.NewIDAddr(t, 1234)
    // rt.SetCaller(someAccountActor, builtin.AccountActorCodeID)

    // uniqueAddr := tutil.NewActorAddr(t, "multisig")

    // rt.SetNewActorAddress(uniqueAddr)
    // // next id address
    // expectedIdAddr := tutil.NewIDAddr(t, 100)
    // rt.ExpectCreateActor(builtin.MultisigActorCodeID, expectedIdAddr)
    // // expect a send to the multisig actor constructor
    // rt.ExpectSend(expectedIdAddr, builtin.MethodConstructor, fakeParams, big.Zero(), nil, exitcode.Ok)
    // execRet := actor.execAndVerify(rt, builtin.MultisigActorCodeID, fakeParams)

    // assert_eq!(uniqueAddr, execRet.RobustAddress)
    // assert_eq!(expectedIdAddr, execRet.IDAddress)

    assert_eq!(true, true);
}

#[test]
fn sending_constructor_failure() {
    assert_eq!(true, true);
}

fn construct_and_verify<BS: BlockStore>(rt: &mut MockRuntime<'_, BS>) {
    rt.expect_validate_caller_addr(&[SYSTEM_ACTOR_ADDR.clone()]);
    let params = ConstructorParams {
        network_name:"mock".to_string()
    };
    let ret = rt.call(
                &*INIT_ACTOR_CODE_ID,
                1,
                &Serialized::serialize(&params).unwrap(),
            ).unwrap();

    let initial_state = Serialized::default();
    assert_eq!(initial_state, ret);
    rt.verify();


    let state_data = match rt.get_state() {
        Err(E) => assert_eq!(true, false,"Failed to get State Data"),
        Ok(T)  => T
    };
    //fn store(&self) -> &BS {

    //fn make_map<BS: BlockStore>(store: &'_ BS) -> Hamt<'_, BytesKey, BS> {
    //Remaining Go lines to port over
    //emptyMap, err := adt.AsMap(adt.AsStore(rt), st.AddressMap)
    //assert.NoError(h.t, err)
    //assert.Equal(h.t, tutil.MustRoot(h.t, emptyMap), st.AddressMap)
    //assert.Equal(h.t, abi.ActorID(builtin.FirstNonSingletonActorId), st.NextID)

    //assert_eq!("mock".to_string(), state_data.NetworkName)
}

fn exec_and_verify<BS: BlockStore>(rt: &mut MockRuntime<'_, BS>, code_id : Cid ,  params: &ConstructorParams) -> Result<Serialized, ActorError>{
    rt.expect_validate_caller_any();
    let exec_params = ExecParams {
        code_cid: code_id,
        constructor_params: Serialized::serialize(&params).unwrap()
    };

    let ret = rt.call(
        &*INIT_ACTOR_CODE_ID,
        2,
        &Serialized::serialize(&exec_params).unwrap()
    );
    rt.verify();
    ret
}
