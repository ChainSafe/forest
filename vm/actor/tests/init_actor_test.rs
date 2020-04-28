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
use cid::{multihash::Identity, Cid, Codec};
use common::*;
use db::MemoryDB;
use ipld_blockstore::BlockStore;
use message::UnsignedMessage;
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

    //Set caller
    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), anne);

    rt.new_actor_addr = Some(Address::new_id(1001).unwrap());
    rt.actor_code_cids.insert(
        Address::new_id(1001).unwrap(),
        ACCOUNT_ACTOR_CODE_ID.clone(),
    );

    let error = exec_and_verify(
        &mut rt,
        POWER_ACTOR_CODE_ID.clone(),
        &ConstructorParams {
            network_name: String::new(),
        },
    )
    .unwrap_err();

    let error_exit_code = error.exit_code();
    assert_eq!(
        error_exit_code,
        ExitCode::ErrForbidden,
        "Error code returned is not ErrForbidden"
    );

    // Didnt see a undef cid like in the go implmentation. If there is replace the not_a_actor token. Need to ask about thi
    // Can porbbaly get rid of this
    // GO implementation has a undef actor so when this test runs it should return illegal actor
    let undef_cid = Cid::new_v1(Codec::Raw, Identity::digest(b"fil/1/notaactor"));
    let error = exec_and_verify(
        &mut rt,
        undef_cid,
        &ConstructorParams {
            network_name: String::new(),
        },
    )
    .expect_err("Exec should have failed");

    let error_exit_code = error.exit_code();
    assert_eq!(
        error_exit_code,
        ExitCode::SysErrorIllegalActor,
        "Error code returned is not SysErrorIllegalActor"
    );
}

#[test]
fn create_2_payment_channels() {
    let bs = MemoryDB::default();
    let mut rt: MockRuntime<MemoryDB> = construct_runtime(&bs);
    construct_and_verify(&mut rt);
    let anne = Address::new_id(1001).unwrap();

    //Set caller
    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), anne);

    // TODO : Change balances not sure how to do i saw the send function, but idk if thats all i need

    // Go test does 2 payment channel tests
    for n in 0..2 {
        //let pay_channel = String::from("paych") + n.to_string();
        let pay_channel_string = format!("paych_{}", n);
        let paych = pay_channel_string.as_bytes();

        let uniq_addr_1 = Address::new_actor(paych);
        rt.new_actor_addr = Some(Address::new_actor(paych).unwrap());

        let expected_id_addr_1 = Address::new_id(100 + n).unwrap();

        rt.expect_create_actor(PAYCH_ACTOR_CODE_ID.clone(), expected_id_addr_1.clone());

        let fake_params = ConstructorParams {
            network_name: String::from("fake_param"),
        };

        // expect anne creating a payment channel to trigger a send to the payment channels constructor
        let balance = TokenAmount::new(vec![1, 0, 0]);

        rt.expect_send(
            expected_id_addr_1.clone(),
            METHOD_CONSTRUCTOR,
            Serialized::serialize(&fake_params).unwrap(),
            balance,
            Serialized::default(),
            ExitCode::Ok,
        );

        let exec_ret = exec_and_verify(&mut rt, PAYCH_ACTOR_CODE_ID.clone(), &fake_params).unwrap();
        let exec_ret: ExecReturn = Serialized::deserialize(&exec_ret).unwrap();
        assert_eq!(
            uniq_addr_1,
            Ok(exec_ret.robust_address),
            "Robust Address does not match"
        );
        assert_eq!(
            expected_id_addr_1, exec_ret.id_address,
            "Id address does not match"
        );

        let state: State = rt.get_state().unwrap();
        let returned_address = state
            .resolve_address(rt.store, &uniq_addr_1.unwrap())
            .expect("Address should have been found");

        assert_eq!(
            returned_address, expected_id_addr_1,
            "Wrong Address returned"
        );
    }
}

#[test]
fn create_storage_miner() {
    let bs = MemoryDB::default();
    let mut rt: MockRuntime<MemoryDB> = construct_runtime(&bs);
    construct_and_verify(&mut rt);

    // // only the storage power actor can create a miner
    rt.set_caller(
        POWER_ACTOR_CODE_ID.clone(),
        STORAGE_POWER_ACTOR_ADDR.clone(),
    );

    let uniq_addr_1 = Address::new_actor(b"miner").unwrap();
    rt.new_actor_addr = Some(uniq_addr_1.clone());

    let expected_id_addr_1 = Address::new_id(100).unwrap();
    rt.expect_create_actor(MINER_ACTOR_CODE_ID.clone(), expected_id_addr_1.clone());

    let fake_params = ConstructorParams {
        network_name: String::from("fake_param"),
    };

    rt.expect_send(
        expected_id_addr_1.clone(),
        METHOD_CONSTRUCTOR,
        Serialized::serialize(&fake_params).unwrap(),
        0u8.into(),
        Serialized::default(),
        ExitCode::Ok,
    );

    let exec_ret = exec_and_verify(&mut rt, MINER_ACTOR_CODE_ID.clone(), &fake_params).unwrap();

    let exec_ret: ExecReturn = Serialized::deserialize(&exec_ret).unwrap();
    assert_eq!(
        uniq_addr_1, exec_ret.robust_address,
        "Robust address does not match"
    );
    assert_eq!(
        expected_id_addr_1, exec_ret.id_address,
        "Id address does not match"
    );

    // Address should be resolved
    let state: State = rt.get_state().unwrap();
    let returned_address = state
        .resolve_address(rt.store, &uniq_addr_1)
        .expect("Address should have been found");
    assert_eq!(returned_address, uniq_addr_1, "Wrong Address returned");

    // Should return error since the address of flurbo is unknown
    let unknown_addr = Address::new_actor(b"flurbo").unwrap();
    let returned_address = state
        .resolve_address(rt.store, &unknown_addr)
        .expect_err("Address should have not been found");
    // Address not found error is returned as string
    assert_eq!(
        returned_address, "Address not found",
        "Wrong Address returned"
    );
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
    let uniq_addr_1 = Address::new_actor(b"multisig").unwrap();
    rt.new_actor_addr = Some(uniq_addr_1.clone());

    // Next id
    let expected_id_addr_1 = Address::new_id(100).unwrap();
    rt.expect_create_actor(MULTISIG_ACTOR_CODE_ID.clone(), expected_id_addr_1.clone());

    let fake_params = ConstructorParams {
        network_name: String::from("fake_param"),
    };
    // Expect a send to the multisig actor constructor
    rt.expect_send(
        expected_id_addr_1.clone(),
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
        uniq_addr_1, exec_ret.robust_address,
        "Robust address does not macth"
    );
    assert_eq!(
        expected_id_addr_1, exec_ret.id_address,
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
    let uniq_addr_1 = Address::new_actor(b"miner").unwrap();
    rt.new_actor_addr = Some(uniq_addr_1.clone());

    // Create the next id address
    let expected_id_addr_1 = Address::new_id(100).unwrap();

    //rt.actor_code_cids.insert(expected_id_addr_1.clone(), POWER_ACTOR_CODE_ID.clone() );
    rt.expect_create_actor(MINER_ACTOR_CODE_ID.clone(), expected_id_addr_1.clone());

    let fake_params = ConstructorParams {
        network_name: String::from("fake_param"),
    };
    rt.expect_send(
        expected_id_addr_1.clone(),
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
        .resolve_address(rt.store, &uniq_addr_1)
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
