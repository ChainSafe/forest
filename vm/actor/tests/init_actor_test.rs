// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod common;

use address::Address;
use cid::Cid;
use common::*;
use fil_types::HAMT_BIT_WIDTH;
use forest_actor::{
    init::{ConstructorParams, ExecParams, ExecReturn, Method, State},
    Multimap, ACCOUNT_ACTOR_CODE_ID, FIRST_NON_SINGLETON_ADDR, INIT_ACTOR_CODE_ID,
    MINER_ACTOR_CODE_ID, MULTISIG_ACTOR_CODE_ID, PAYCH_ACTOR_CODE_ID, POWER_ACTOR_CODE_ID,
    STORAGE_POWER_ACTOR_ADDR, SYSTEM_ACTOR_ADDR, SYSTEM_ACTOR_CODE_ID,
};
use serde::Serialize;
use vm::{ActorError, ExitCode, Serialized, TokenAmount, METHOD_CONSTRUCTOR};

fn construct_runtime() -> MockRuntime {
    MockRuntime {
        receiver: Address::new_id(1000),
        caller: *SYSTEM_ACTOR_ADDR,
        caller_type: SYSTEM_ACTOR_CODE_ID.clone(),
        ..Default::default()
    }
}

// Test to make sure we abort actors that can not call the exec function
#[test]
fn abort_cant_call_exec() {
    let mut rt = construct_runtime();
    construct_and_verify(&mut rt);
    let anne = Address::new_id(1001);

    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), anne);

    let err = exec_and_verify(&mut rt, POWER_ACTOR_CODE_ID.clone(), &"")
        .expect_err("Exec should have failed");
    assert_eq!(err.exit_code(), ExitCode::ErrForbidden);
}

#[test]
fn create_2_payment_channels() {
    let mut rt = construct_runtime();
    construct_and_verify(&mut rt);
    let anne = Address::new_id(1001);

    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), anne);

    for n in 0..2 {
        let pay_channel_string = format!("paych_{}", n);
        let paych = pay_channel_string.as_bytes();

        rt.balance = TokenAmount::from(100);
        rt.value_received = TokenAmount::from(100);

        let unique_address = Address::new_actor(paych);
        rt.new_actor_addr = Some(Address::new_actor(paych));

        let expected_id_addr = Address::new_id(100 + n);
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
            unique_address, exec_ret.robust_address,
            "Robust Address does not match"
        );
        assert_eq!(
            expected_id_addr, exec_ret.id_address,
            "Id address does not match"
        );

        let state: State = rt.get_state().unwrap();
        let returned_address = state
            .resolve_address(&rt.store, &unique_address)
            .expect("Resolve should not error")
            .expect("Address should be able to be resolved");

        assert_eq!(returned_address, expected_id_addr, "Wrong Address returned");
    }
}

#[test]
fn create_storage_miner() {
    let mut rt = construct_runtime();
    construct_and_verify(&mut rt);

    // only the storage power actor can create a miner
    rt.set_caller(
        POWER_ACTOR_CODE_ID.clone(),
        STORAGE_POWER_ACTOR_ADDR.clone(),
    );

    let unique_address = Address::new_actor(b"miner");
    rt.new_actor_addr = Some(unique_address.clone());

    let expected_id_addr = Address::new_id(100);
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
        .resolve_address(&rt.store, &unique_address)
        .expect("Resolve should not error")
        .expect("Address should be able to be resolved");
    assert_eq!(expected_id_addr, returned_address);

    // Should return error since the address of flurbo is unknown
    let unknown_addr = Address::new_actor(b"flurbo");

    let returned_address = state.resolve_address(&rt.store, &unknown_addr).unwrap();
    assert_eq!(
        returned_address, None,
        "Addresses should have not been found"
    );
}

#[test]
fn create_multisig_actor() {
    let mut rt = construct_runtime();
    construct_and_verify(&mut rt);

    // Actor creating multisig actor
    let some_acc_actor = Address::new_id(1234);
    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), some_acc_actor);

    // Assign addresses
    let unique_address = Address::new_actor(b"multisig");
    rt.new_actor_addr = Some(unique_address.clone());

    // Next id
    let expected_id_addr = Address::new_id(100);
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
    let mut rt = construct_runtime();
    construct_and_verify(&mut rt);

    // Only the storage power actor can create a miner
    rt.set_caller(
        POWER_ACTOR_CODE_ID.clone(),
        STORAGE_POWER_ACTOR_ADDR.clone(),
    );

    // Assign new address for the storage actor miner
    let unique_address = Address::new_actor(b"miner");
    rt.new_actor_addr = Some(unique_address.clone());

    // Create the next id address
    let expected_id_addr = Address::new_id(100);
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

    let error = exec_and_verify(&mut rt, MINER_ACTOR_CODE_ID.clone(), &fake_params)
        .expect_err("sending constructor should have failed");

    let error_exit_code = error.exit_code();

    assert_eq!(
        error_exit_code,
        ExitCode::ErrIllegalState,
        "Exit Code that is returned is not ErrIllegalState"
    );

    let state: State = rt.get_state().unwrap();

    let returned_address = state.resolve_address(&rt.store, &unique_address).unwrap();
    assert_eq!(
        returned_address, None,
        "Addresses should have not been found"
    );
}

fn construct_and_verify(rt: &mut MockRuntime) {
    rt.expect_validate_caller_addr(vec![SYSTEM_ACTOR_ADDR.clone()]);
    let params = ConstructorParams {
        network_name: "mock".to_string(),
    };
    let ret = rt
        .call(
            &*INIT_ACTOR_CODE_ID,
            METHOD_CONSTRUCTOR,
            &Serialized::serialize(&params).unwrap(),
        )
        .unwrap();

    assert_eq!(Serialized::default(), ret);
    rt.verify();

    let state_data: State = rt.get_state().unwrap();

    // Gets the Result(CID)
    let empty_map = Multimap::from_root(&rt.store, &state_data.address_map, HAMT_BIT_WIDTH, 3)
        .unwrap()
        .root();

    assert_eq!(empty_map.unwrap(), state_data.address_map);
    assert_eq!(FIRST_NON_SINGLETON_ADDR, state_data.next_id);
    assert_eq!("mock".to_string(), state_data.network_name);
}

fn exec_and_verify<'a, S: Serialize>(
    rt: &mut MockRuntime,
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

    let ret = rt.call(
        &*INIT_ACTOR_CODE_ID,
        Method::Exec as u64,
        &Serialized::serialize(&exec_params).unwrap(),
    );

    rt.verify();
    ret
}
