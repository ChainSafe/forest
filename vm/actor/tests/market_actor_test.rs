// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod common;

use actor::{
    init::{ConstructorParams, ExecParams, ExecReturn},
    market::{DealState, Method, State, WithdrawBalanceParams},
    miner::MinerInfo,
    BalanceTable, Multimap, SetMultimap, ACCOUNT_ACTOR_CODE_ID, FIRST_NON_SINGLETON_ADDR,
    INIT_ACTOR_CODE_ID, MARKET_ACTOR_CODE_ID, MINER_ACTOR_CODE_ID, MULTISIG_ACTOR_CODE_ID,
    PAYCH_ACTOR_CODE_ID, POWER_ACTOR_CODE_ID, STORAGE_MARKET_ACTOR_ADDR, STORAGE_POWER_ACTOR_ADDR,
    SYSTEM_ACTOR_ADDR, SYSTEM_ACTOR_CODE_ID,
};
use address::Address;
use cid::Cid;
use common::*;
use db::MemoryDB;
use ipld_amt::Amt;
use ipld_blockstore::BlockStore;
use message::{Message, UnsignedMessage};
use serde::{de::DeserializeOwned, Serialize};
use vm::{
    ActorError, ExitCode, SectorSize, Serialized, TokenAmount, METHOD_CONSTRUCTOR, METHOD_SEND,
};

enum TestId {
    MarketActorId = 100,
    OwnerId = 101,
    ProviderId = 102,
    WorkerId = 103,
    ClientId = 104,
}

//const caller_types_signable : Vec<Cid>= vec![ACCOUNT_ACTOR_CODE_ID.clone(), MULTISIG_ACTOR_CODE_ID.clone()];

fn setup<BS: BlockStore>(bs: &BS) -> MockRuntime<'_, BS> {
    let message = UnsignedMessage::builder()
        .to(*STORAGE_MARKET_ACTOR_ADDR)
        .from(*SYSTEM_ACTOR_ADDR)
        .build()
        .unwrap();

    let mut rt = MockRuntime::new(bs, message);

    rt.caller_type = INIT_ACTOR_CODE_ID.clone();

    rt.actor_code_cids.insert(
        Address::new_id(TestId::OwnerId as u64),
        ACCOUNT_ACTOR_CODE_ID.clone(),
    );
    rt.actor_code_cids.insert(
        Address::new_id(TestId::WorkerId as u64),
        ACCOUNT_ACTOR_CODE_ID.clone(),
    );
    rt.actor_code_cids.insert(
        Address::new_id(TestId::ProviderId as u64),
        MINER_ACTOR_CODE_ID.clone(),
    );
    rt.actor_code_cids.insert(
        Address::new_id(TestId::ClientId as u64),
        ACCOUNT_ACTOR_CODE_ID.clone(),
    );
    construct_and_verify(&mut rt);

    rt
}

// TODO add array stuff
#[test]
fn simple_construction() {
    let bs = MemoryDB::default();

    let receiver: Address = Address::new_id(100);

    let message = UnsignedMessage::builder()
        .to(receiver.clone())
        .from(*SYSTEM_ACTOR_ADDR)
        .build()
        .unwrap();

    let mut rt = MockRuntime::new(&bs, message);
    rt.caller_type = INIT_ACTOR_CODE_ID.clone();

    rt.expect_validate_caller_addr(&[SYSTEM_ACTOR_ADDR.clone()]);

    let call_result = rt
        .call(
            &*MARKET_ACTOR_CODE_ID,
            METHOD_CONSTRUCTOR,
            &Serialized::default(),
        )
        .unwrap();

    assert_eq!(call_result, Serialized::default());

    rt.verify();

    let store = rt.store;
    let empty_map = Multimap::new(store).root().unwrap();
    let empty_set = SetMultimap::new(store).root().unwrap();

    let state_data: State = rt.get_state().unwrap();

    assert_eq!(empty_map, state_data.escrow_table);
    assert_eq!(empty_map, state_data.locked_table);
    assert_eq!(empty_set, state_data.deal_ids_by_party);
    assert_eq!( state_data.last_cron.is_none(), true );
}

//#[test]
fn add_provider_escrow_funds() {
    // First element of tuple is the delta the second element is the total after the delta change
    let test_cases = vec![(10, 10), (20, 30), (40, 70)];

    let owner_addr = Address::new_id(TestId::OwnerId as u64);
    let worker_addr = Address::new_id(TestId::WorkerId as u64);
    let provider_addr = Address::new_id(TestId::ProviderId as u64);

    for caller_addr in vec![owner_addr, worker_addr] {
        let bs = MemoryDB::default();
        let mut rt = setup(&bs);

        for test_case in test_cases.clone() {
            rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), caller_addr);

            let amount = TokenAmount::from(test_case.0 as u64);
            //rt.balance = rt.balance + amount.clone();
            rt.set_value(amount);

            expect_provider_control_address(&mut rt, provider_addr, owner_addr, worker_addr);

            rt.call(
                &MARKET_ACTOR_CODE_ID.clone(),
                Method::AddBalance as u64,
                &Serialized::serialize(provider_addr.clone()).unwrap(),
            ) ;
            rt.verify();

            let state_data: State = rt.get_state().unwrap();
            assert_eq!(
                state_data
                    .get_escrow_balance(rt.store, &provider_addr)
                    .unwrap(),
                TokenAmount::from(test_case.1 as u64)
            );
        }
    }
}

//#[test]
fn account_actor_check() {
    let bs = MemoryDB::default();
    let mut rt = setup(&bs);

    let amount = TokenAmount::from(10u8);
    rt.set_value(amount);

    let owner_addr = Address::new_id(TestId::OwnerId as u64);
    let worker_addr = Address::new_id(TestId::WorkerId as u64);
    let provider_addr = Address::new_id(TestId::ProviderId as u64);

    expect_provider_control_address(&mut rt, provider_addr, owner_addr, worker_addr);
    rt.set_caller(MINER_ACTOR_CODE_ID.clone(), provider_addr.clone());
    let error_code = rt
        .call(
            &MARKET_ACTOR_CODE_ID.clone(),
            Method::AddBalance as u64,
            &Serialized::serialize(provider_addr).unwrap(),
        )
        .unwrap_err()
        .exit_code();

    assert_eq!(error_code, ExitCode::ErrForbidden);

    rt.verify();
}

//#[test]
fn add_non_provider_funds() {
    // First element of tuple is the delta the second element is the total after the delta change
    let test_cases = vec![(10, 10), (20, 30), (40, 70)];

    let client_addr = Address::new_id(TestId::ClientId as u64);
    let worker_addr = Address::new_id(TestId::WorkerId as u64);

    for caller_addr in vec![client_addr, worker_addr] {
        let bs = MemoryDB::default();
        let mut rt = setup(&bs);

        for test_case in test_cases.clone() {
            rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), caller_addr);

            let amount = TokenAmount::from(test_case.0 as u64);
            //rt.balance = rt.balance + amount.clone();
            rt.set_value(amount);
            rt.expect_validate_caller_type(&[]);

            rt.call(
                &MARKET_ACTOR_CODE_ID.clone(),
                Method::AddBalance as u64,
                &Serialized::serialize(caller_addr.clone()).unwrap(),
            ) ;

            rt.verify();

            let state_data: State = rt.get_state().unwrap();
            assert_eq!(
                state_data
                    .get_escrow_balance(rt.store, &caller_addr)
                    .unwrap(),
                TokenAmount::from(test_case.1 as u8)
            );
        }
    }
}

//#[test]
fn withdraw_provider_to_owner() {
    let bs = MemoryDB::default();
    let mut rt = setup(&bs);

    let owner_addr = Address::new_id(TestId::OwnerId as u64);
    let worker_addr = Address::new_id(TestId::WorkerId as u64);
    let provider_addr = Address::new_id(TestId::ProviderId as u64);

    let amount = TokenAmount::from(20u8);
    add_provider_funds(
        &mut rt,
        provider_addr.clone(),
        owner_addr.clone(),
        worker_addr.clone(),
        amount.clone(),
    );

    let state_data: State = rt.get_state().unwrap();
    assert_eq!(
        amount,
        state_data
            .get_escrow_balance(rt.store, &provider_addr)
            .unwrap()
    );

    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), worker_addr.clone());
    expect_provider_control_address(&mut rt, provider_addr, owner_addr, worker_addr);

    let withdraw_amount = TokenAmount::from(1u8);

    rt.expect_send(
        owner_addr.clone(),
        METHOD_SEND,
        Serialized::default(),
        withdraw_amount.clone(),
        Serialized::default(),
        ExitCode::Ok,
    );

    let params = WithdrawBalanceParams {
        provider_or_client: provider_addr.clone(),
        amount: withdraw_amount.clone(),
    };

    let call_result = rt.call(
        &MARKET_ACTOR_CODE_ID.clone(),
        Method::WithdrawBalance as u64,
        &Serialized::serialize(params).unwrap(),
    );

    rt.verify();

    let state_data: State = rt.get_state().unwrap();

    assert_eq!(
        state_data
            .get_escrow_balance(rt.store, &provider_addr)
            .unwrap(),
        TokenAmount::from(19u8)
    );
}

#[test]
fn withdraw_non_provider() {
    // Test is currently failing because curr_epoch  is 0. When subtracted by 1, it goe snmegative causing a overflow error
    let bs = MemoryDB::default();
    let mut rt = setup(&bs);

    let client_addr = Address::new_id(TestId::ClientId as u64);

    let amount = TokenAmount::from(20u8);
    add_participant_funds(&mut rt, client_addr.clone(), amount.clone());

    let state_data: State = rt.get_state().unwrap();
    assert_eq!(
        amount,
        state_data
            .get_escrow_balance(rt.store, &client_addr)
            .unwrap()
    );

    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), client_addr.clone());
    rt.expect_validate_caller_type(&[
        ACCOUNT_ACTOR_CODE_ID.clone(),
        MULTISIG_ACTOR_CODE_ID.clone(),
    ]);

    let withdraw_amount = TokenAmount::from(1u8);

    rt.expect_send(
        client_addr.clone(),
        METHOD_SEND,
        Serialized::default(),
        withdraw_amount.clone(),
        Serialized::default(),
        ExitCode::Ok,
    );

    let params = WithdrawBalanceParams {
        provider_or_client: client_addr.clone(),
        amount: withdraw_amount.clone(),
    };

    let call_result = rt.call(
        &MARKET_ACTOR_CODE_ID.clone(),
        Method::WithdrawBalance as u64,
        &Serialized::serialize(params).unwrap(),
    );

    rt.verify();

    let state_data: State = rt.get_state().unwrap();

    assert_eq!(
        state_data
            .get_escrow_balance(rt.store, &client_addr)
            .unwrap(),
        TokenAmount::from(19u8)
    );
}

#[test]
fn client_withdraw_more_than_available() {
    let bs = MemoryDB::default();
    let mut rt = setup(&bs);

    let client_addr = Address::new_id(TestId::ClientId as u64);

    let amount = TokenAmount::from(20u8);
    add_participant_funds(&mut rt, client_addr.clone(), amount.clone());

    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), client_addr.clone());
    rt.expect_validate_caller_type(&[
        ACCOUNT_ACTOR_CODE_ID.clone(),
        MULTISIG_ACTOR_CODE_ID.clone(),
    ]);

    let withdraw_amount = TokenAmount::from(25u8);

    rt.expect_send(
        client_addr.clone(),
        METHOD_SEND,
        Serialized::default(),
        amount.clone(),
        Serialized::default(),
        ExitCode::Ok,
    );

    let params = WithdrawBalanceParams {
        provider_or_client: client_addr.clone(),
        amount: withdraw_amount.clone(),
    };

    let call_result = rt.call(
        &MARKET_ACTOR_CODE_ID.clone(),
        Method::WithdrawBalance as u64,
        &Serialized::serialize(params).unwrap(),
    );

    rt.verify();

    let state_data: State = rt.get_state().unwrap();

    assert_eq!(
        state_data
            .get_escrow_balance(rt.store, &client_addr)
            .unwrap(),
        TokenAmount::from(0u8)
    );
}

//#[test]
fn worker_withdraw_more_than_available() {
    let bs = MemoryDB::default();
    let mut rt = setup(&bs);

    let owner_addr = Address::new_id(TestId::OwnerId as u64);
    let worker_addr = Address::new_id(TestId::WorkerId as u64);
    let provider_addr = Address::new_id(TestId::ProviderId as u64);

    let amount = TokenAmount::from(20u8);
    add_provider_funds(
        &mut rt,
        provider_addr.clone(),
        owner_addr.clone(),
        worker_addr.clone(),
        amount.clone(),
    );

    let state_data: State = rt.get_state().unwrap();
    assert_eq!(
        amount,
        state_data
            .get_escrow_balance(rt.store, &provider_addr)
            .unwrap()
    );

    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), worker_addr.clone());
    expect_provider_control_address(&mut rt, provider_addr, owner_addr, worker_addr);

    let withdraw_amount = TokenAmount::from(25u8);

    rt.expect_send(
        owner_addr.clone(),
        METHOD_SEND,
        Serialized::default(),
        amount.clone(),
        Serialized::default(),
        ExitCode::Ok,
    );

    let params = WithdrawBalanceParams {
        provider_or_client: provider_addr.clone(),
        amount: withdraw_amount.clone(),
    };

    let call_result = rt.call(
        &MARKET_ACTOR_CODE_ID.clone(),
        Method::WithdrawBalance as u64,
        &Serialized::serialize(params).unwrap(),
    );

    rt.verify();

    let state_data: State = rt.get_state().unwrap();

    assert_eq!(
        state_data
            .get_escrow_balance(rt.store, &provider_addr)
            .unwrap(),
        TokenAmount::from(0u8)
    );
}

fn expect_provider_control_address<BS: BlockStore>(
    rt: &mut MockRuntime<'_, BS>,
    provider: Address,
    owner: Address,
    worker: Address,
) {
    rt.expect_validate_caller_addr(&[owner.clone(), worker.clone()]);

    // TODO Provide the right methjod number. THe right mehtod is controlAddress in go code
    // rt.expect_send(provider.clone(),
    //  METHOD_CONSTRUCTOR,
    //  Serialized::default(),
    //  TokenAmount::from(0u8),
    //  Serialized::serialize((owner.clone(), worker.clone())).unwrap(),
    //  ExitCode::Ok
    // );
}

fn add_provider_funds<BS: BlockStore>(
    rt: &mut MockRuntime<'_, BS>,
    provider: Address,
    owner: Address,
    worker: Address,
    amount: TokenAmount,
) {
    rt.set_value(amount.clone());

    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), owner.clone());
    expect_provider_control_address(rt, provider, owner, worker);

    let v = rt.call(
        &MARKET_ACTOR_CODE_ID.clone(),
        Method::AddBalance as u64,
        &Serialized::serialize(provider.clone()).unwrap(),
    );

    rt.verify();

    rt.balance = rt.balance.clone() + amount;
}

fn add_participant_funds<BS: BlockStore>(
    rt: &mut MockRuntime<'_, BS>,
    addr: Address,
    amount: TokenAmount,
) {
    rt.set_value(amount.clone());

    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), addr.clone());

    rt.expect_validate_caller_type(&[
        ACCOUNT_ACTOR_CODE_ID.clone(),
        MULTISIG_ACTOR_CODE_ID.clone(),
    ]);

    let v = rt.call(
        &MARKET_ACTOR_CODE_ID.clone(),
        Method::AddBalance as u64,
        &Serialized::serialize(addr.clone()).unwrap(),
    );

    rt.verify();

    rt.balance = rt.balance.clone() + amount;
}

fn construct_and_verify<BS: BlockStore>(rt: &mut MockRuntime<'_, BS>) {
    rt.expect_validate_caller_addr(&[SYSTEM_ACTOR_ADDR.clone()]);

    let ret = rt
        .call(
            &*MARKET_ACTOR_CODE_ID,
            METHOD_CONSTRUCTOR,
            &Serialized::default(),
        )
        .unwrap();

    assert_eq!(Serialized::default(), ret);
    rt.verify();
}
