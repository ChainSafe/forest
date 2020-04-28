// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod common;

use actor::{
    init::{ConstructorParams, ExecParams, ExecReturn, State},
    Multimap, ACCOUNT_ACTOR_CODE_ID, FIRST_NON_SINGLETON_ADDR, INIT_ACTOR_CODE_ID,
    MINER_ACTOR_CODE_ID, MULTISIG_ACTOR_CODE_ID, PAYCH_ACTOR_CODE_ID, POWER_ACTOR_CODE_ID,
    STORAGE_POWER_ACTOR_ADDR, SYSTEM_ACTOR_ADDR, SYSTEM_ACTOR_CODE_ID,
};
use address::Address;
use cid::Cid;
use common::*;
use db::MemoryDB;
use ipld_blockstore::BlockStore;
use message::UnsignedMessage;
use serde::Serialize;
use vm::{ActorError, ExitCode, Serialized, TokenAmount, METHOD_CONSTRUCTOR};

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
    let anne = Address::new_id(1001).unwrap();

    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), anne);

    let err = exec_and_verify(&mut rt, POWER_ACTOR_CODE_ID.clone(), &"")
        .expect_err("Exec should have failed");
    assert_eq!(err.exit_code(), ExitCode::ErrForbidden);
}

#[test]
fn create_2_payment_channels() {
    let bs = MemoryDB::default();
    let mut rt: MockRuntime<MemoryDB> = construct_runtime(&bs);
    construct_and_verify(&mut rt);
    let anne = Address::new_id(1001).unwrap();

    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), anne);

    for n in 0..2 {
        let pay_channel_string = format!("paych_{}", n);
        let paych = pay_channel_string.as_bytes();

        rt.balance = TokenAmount::from(100u8);
        rt.value_received = TokenAmount::from(100u8);

        let unique_address = Address::new_actor(paych);
        rt.new_actor_addr = Some(Address::new_actor(paych).unwrap());

        let expected_id_addr = Address::new_id(100 + n).unwrap();
        rt.expect_create_actor(PAYCH_ACTOR_CODE_ID.clone(), expected_id_addr.clone());

        let fake_params = ConstructorParams {
            network_name: String::from("fake_param"),
        };

        // expect anne creating a payment channel to trigger a send to the payment channels constructor
        let balance = TokenAmount::from(100u8);

        rt.expect_send(
            expected_id_addr.clone(),
            METHOD_CONSTRUCTOR,
            Serialized::serialize(&fake_params).unwrap(),
            balance,
            Serialized::default(),
            ExitCode::Ok,
        );

        let exec_ret = exec_and_verify(&mut rt, PAYCH_ACTOR_CODE_ID.clone(), &fake_params).unwrap();
        let exec_ret: ExecReturn = Serialized::deserialize(&exec_ret).unwrap();
        assert_eq!(
            unique_address,
            Ok(exec_ret.robust_address),
            "Robust Address does not match"
        );
        assert_eq!(
            expected_id_addr, exec_ret.id_address,
            "Id address does not match"
        );

        let state: State = rt.get_state().unwrap();
        let returned_address = state
            .resolve_address(rt.store, &unique_address.unwrap())
            .expect("Address should have been found");

        assert_eq!(returned_address, expected_id_addr, "Wrong Address returned");
    }
}

#[test]
fn create_storage_miner() {
    let bs = MemoryDB::default();
    let mut rt: MockRuntime<MemoryDB> = construct_runtime(&bs);
    construct_and_verify(&mut rt);

    // only the storage power actor can create a miner
    rt.set_caller(
        POWER_ACTOR_CODE_ID.clone(),
        STORAGE_POWER_ACTOR_ADDR.clone(),
    );

    let unique_address = Address::new_actor(b"miner").unwrap();
    rt.new_actor_addr = Some(unique_address.clone());

    let expected_id_addr = Address::new_id(100).unwrap();
    rt.expect_create_actor(MINER_ACTOR_CODE_ID.clone(), expected_id_addr.clone());

    let fake_params = ConstructorParams {
        network_name: String::from("fake_param"),
    };

    rt.expect_send(
        expected_id_addr.clone(),
        METHOD_CONSTRUCTOR,
        Serialized::serialize(&fake_params).unwrap(),
        0u8.into(),
        Serialized::default(),
        ExitCode::Ok,
    );

    let exec_ret = exec_and_verify(&mut rt, MINER_ACTOR_CODE_ID.clone(), &fake_params).unwrap();

    let exec_ret: ExecReturn = Serialized::deserialize(&exec_ret).unwrap();
    assert_eq!(unique_address, exec_ret.robust_address);
    assert_eq!(expected_id_addr, exec_ret.id_address);

    // Address should be resolved
    let state: State = rt.get_state().unwrap();
    let returned_address = state
        .resolve_address(rt.store, &unique_address)
        .expect("Address should have been found");
    assert_eq!(expected_id_addr, returned_address);

    // Should return error since the address of flurbo is unknown
    let unknown_addr = Address::new_actor(b"flurbo").unwrap();
    state
        .resolve_address(rt.store, &unknown_addr)
        .expect_err("Address should have not been found");
}

#[test]
fn create_multisig_actor() {
    let bs = MemoryDB::default();
    let mut rt = construct_runtime(&bs);
    construct_and_verify(&mut rt);

    // Actor creating multisig actor
    let some_acc_actor = Address::new_id(1234).unwrap();
    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), some_acc_actor);

    //Assign addresses
    let unique_address = Address::new_actor(b"multisig").unwrap();
    rt.new_actor_addr = Some(unique_address.clone());

    // Next id
    let expected_id_addr = Address::new_id(100).unwrap();
    rt.expect_create_actor(MULTISIG_ACTOR_CODE_ID.clone(), expected_id_addr.clone());

    let fake_params = ConstructorParams {
        network_name: String::from("fake_param"),
    };
    // Expect a send to the multisig actor constructor
    rt.expect_send(
        expected_id_addr.clone(),
        METHOD_CONSTRUCTOR,
        Serialized::serialize(&fake_params).unwrap(),
        0u8.into(),
        Serialized::default(),
        ExitCode::Ok,
    );

    // Return should have been successful. Check the returned addresses
    let exec_ret = exec_and_verify(&mut rt, MULTISIG_ACTOR_CODE_ID.clone(), &fake_params).unwrap();
    let exec_ret: ExecReturn = Serialized::deserialize(&exec_ret).unwrap();
    assert_eq!(
        unique_address, exec_ret.robust_address,
        "Robust address does not macth"
    );
    assert_eq!(
        expected_id_addr, exec_ret.id_address,
        "Id address does not match"
    );
}

#[test]
fn sending_constructor_failure() {
    let bs = MemoryDB::default();
    let mut rt = construct_runtime(&bs);
    construct_and_verify(&mut rt);

    // Only the storage power actor can create a miner
    rt.set_caller(
        POWER_ACTOR_CODE_ID.clone(),
        STORAGE_POWER_ACTOR_ADDR.clone(),
    );

    //Assign new address for the storage actor miner
    let unique_address = Address::new_actor(b"miner").unwrap();
    rt.new_actor_addr = Some(unique_address.clone());

    // Create the next id address
    let expected_id_addr = Address::new_id(100).unwrap();

    //rt.actor_code_cids.insert(expected_id_addr.clone(), POWER_ACTOR_CODE_ID.clone() );
    rt.expect_create_actor(MINER_ACTOR_CODE_ID.clone(), expected_id_addr.clone());

    let fake_params = ConstructorParams {
        network_name: String::from("fake_param"),
    };
    rt.expect_send(
        expected_id_addr.clone(),
        METHOD_CONSTRUCTOR,
        Serialized::serialize(&fake_params).unwrap(),
        0u8.into(),
        Serialized::default(),
        ExitCode::ErrIllegalState.clone(),
    );

    // Only thr storage power actor can create a storage miner. Init actor creating it should result in failure
    let error = exec_and_verify(&mut rt, MINER_ACTOR_CODE_ID.clone(), &fake_params)
        .expect_err("sending constructor should have failed");
    let error_exit_code = error.exit_code();
    assert_eq!(
        error_exit_code,
        ExitCode::ErrIllegalState,
        "Exit Code that is returned is not ErrIllegalState"
    );

    // The send command from earlier should have failed. So you shouldnt be able to see the address
    let state: State = rt.get_state().unwrap();
    let returned_address = state
        .resolve_address(rt.store, &unique_address)
        .expect_err("Address resolution should have failed");
    // Error is returned as string. Doing it in the lazy way for this PR
    assert_eq!(
        returned_address, "Address not found",
        "Addresses should have not been found"
    );
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

    assert_eq!(Serialized::default(), ret);
    rt.verify();

    let state_data: State = rt.get_state().unwrap();

    // Gets the Result(CID)
    let empty_map = Multimap::from_root(rt.store, &state_data.address_map)
        .unwrap()
        .root();

    assert_eq!(empty_map.unwrap(), state_data.address_map);
    assert_eq!(FIRST_NON_SINGLETON_ADDR, state_data.next_id);
    assert_eq!("mock".to_string(), state_data.network_name);
}

fn exec_and_verify<BS: BlockStore, S: Serialize>(
    rt: &mut MockRuntime<'_, BS>,
    code_id: Cid,
    params: &S,
) -> Result<Serialized, ActorError>
where
    S: Serialize,
{
    rt.expect_validate_caller_any();
    let exec_params = ExecParams {
        code_cid: code_id,
        constructor_params: Serialized::serialize(params).unwrap(),
    };

    rt.message = UnsignedMessage::builder()
        .to(rt.receiver.clone())
        .from(rt.caller.clone())
        .value(rt.value_received.clone())
        .build()
        .unwrap();

    let ret = rt.call(
        &*INIT_ACTOR_CODE_ID,
        2,
        &Serialized::serialize(&exec_params).unwrap(),
    );
    println!("{:?}", ret);

    rt.verify();
    ret
}
