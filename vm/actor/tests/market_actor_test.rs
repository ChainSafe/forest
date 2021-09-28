// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod common;

use address::Address;
use clock::EPOCH_UNDEFINED;
use common::*;
use fil_types::HAMT_BIT_WIDTH;
use forest_actor::{
    make_empty_map,
    market::{Method, State, WithdrawBalanceParams, PROPOSALS_AMT_BITWIDTH, STATES_AMT_BITWIDTH},
    miner::{GetControlAddressesReturn, Method as MinerMethod},
    util::BALANCE_TABLE_BITWIDTH,
    BalanceTable, SetMultimap, ACCOUNT_ACTOR_CODE_ID, CALLER_TYPES_SIGNABLE, INIT_ACTOR_CODE_ID,
    MARKET_ACTOR_CODE_ID, MINER_ACTOR_CODE_ID, MULTISIG_ACTOR_CODE_ID, STORAGE_MARKET_ACTOR_ADDR,
    SYSTEM_ACTOR_ADDR,
};
use ipld_amt::Amt;
use num_bigint::bigint_ser::BigIntDe;
use runtime::Runtime;
use std::collections::HashMap;
use vm::{ActorError, ExitCode, Serialized, TokenAmount, METHOD_CONSTRUCTOR, METHOD_SEND};

const OWNER_ID: u64 = 101;
const PROVIDER_ID: u64 = 102;
const WORKER_ID: u64 = 103;
const CLIENT_ID: u64 = 104;

fn setup() -> MockRuntime {
    let mut actor_code_cids = HashMap::default();
    actor_code_cids.insert(Address::new_id(OWNER_ID), ACCOUNT_ACTOR_CODE_ID.clone());
    actor_code_cids.insert(Address::new_id(WORKER_ID), ACCOUNT_ACTOR_CODE_ID.clone());
    actor_code_cids.insert(Address::new_id(PROVIDER_ID), MINER_ACTOR_CODE_ID.clone());
    actor_code_cids.insert(Address::new_id(CLIENT_ID), ACCOUNT_ACTOR_CODE_ID.clone());

    let mut rt = MockRuntime {
        receiver: *STORAGE_MARKET_ACTOR_ADDR,
        caller: *SYSTEM_ACTOR_ADDR,
        caller_type: INIT_ACTOR_CODE_ID.clone(),
        actor_code_cids,
        ..Default::default()
    };
    construct_and_verify(&mut rt);

    rt
}

fn get_escrow_balance(rt: &MockRuntime, addr: &Address) -> Result<TokenAmount, ActorError> {
    let st: State = rt.get_state()?;

    let et = BalanceTable::from_root(rt.store(), &st.escrow_table).unwrap();

    Ok(et.get(addr).unwrap().clone())
}

// TODO add array stuff
#[test]
fn simple_construction() {
    let mut rt = MockRuntime {
        receiver: Address::new_id(100),
        caller: *SYSTEM_ACTOR_ADDR,
        caller_type: INIT_ACTOR_CODE_ID.clone(),
        ..Default::default()
    };

    rt.expect_validate_caller_addr(vec![SYSTEM_ACTOR_ADDR.clone()]);

    assert_eq!(
        Serialized::default(),
        rt.call(
            &*MARKET_ACTOR_CODE_ID,
            METHOD_CONSTRUCTOR,
            &Serialized::default(),
        )
        .unwrap()
    );

    rt.verify();

    let store = &rt.store;

    let empty_balance_table = make_empty_map::<_, BigIntDe>(store, BALANCE_TABLE_BITWIDTH)
        .flush()
        .unwrap();
    let empty_map = make_empty_map::<_, ()>(store, HAMT_BIT_WIDTH)
        .flush()
        .unwrap();
    let empty_proposals_array = Amt::<(), _>::new_with_bit_width(store, PROPOSALS_AMT_BITWIDTH)
        .flush()
        .unwrap();
    let empty_states_array = Amt::<(), _>::new_with_bit_width(store, STATES_AMT_BITWIDTH)
        .flush()
        .unwrap();
    let empty_multimap = SetMultimap::new(store).root().unwrap();

    let state_data: State = rt.get_state().unwrap();

    assert_eq!(empty_proposals_array, state_data.proposals);
    assert_eq!(empty_states_array, state_data.states);
    assert_eq!(empty_map, state_data.pending_proposals);
    assert_eq!(empty_balance_table, state_data.escrow_table);
    assert_eq!(empty_balance_table, state_data.locked_table);
    assert_eq!(0, state_data.next_id);
    assert_eq!(empty_multimap, state_data.deal_ops_by_epoch);
    assert_eq!(state_data.last_cron, EPOCH_UNDEFINED);
}

#[ignore]
#[test]
fn add_provider_escrow_funds() {
    // First element of tuple is the delta the second element is the total after the delta change
    let test_cases = vec![(10, 10), (20, 30), (40, 70)];

    let owner_addr = Address::new_id(OWNER_ID);
    let worker_addr = Address::new_id(WORKER_ID);
    let provider_addr = Address::new_id(PROVIDER_ID);

    for caller_addr in vec![owner_addr, worker_addr] {
        let mut rt = setup();

        for test_case in test_cases.clone() {
            rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), caller_addr);

            let amount = TokenAmount::from(test_case.0 as u64);
            // rt.balance = rt.balance + amount.clone();
            rt.set_value(amount);

            expect_provider_control_address(&mut rt, provider_addr, owner_addr, worker_addr);

            assert!(rt
                .call(
                    &MARKET_ACTOR_CODE_ID.clone(),
                    Method::AddBalance as u64,
                    &Serialized::serialize(provider_addr.clone()).unwrap(),
                )
                .is_ok());
            rt.verify();

            assert_eq!(
                get_escrow_balance(&rt, &provider_addr).unwrap(),
                TokenAmount::from(test_case.1 as u64)
            );
        }
    }
}

#[ignore]
#[test]
fn account_actor_check() {
    let mut rt = setup();

    let amount = TokenAmount::from(10u8);
    rt.set_value(amount);

    let owner_addr = Address::new_id(OWNER_ID);
    let worker_addr = Address::new_id(WORKER_ID);
    let provider_addr = Address::new_id(PROVIDER_ID);

    expect_provider_control_address(&mut rt, provider_addr, owner_addr, worker_addr);
    rt.set_caller(MINER_ACTOR_CODE_ID.clone(), provider_addr.clone());

    assert_eq!(
        ExitCode::ErrForbidden,
        rt.call(
            &MARKET_ACTOR_CODE_ID.clone(),
            Method::AddBalance as u64,
            &Serialized::serialize(provider_addr).unwrap(),
        )
        .unwrap_err()
        .exit_code()
    );

    rt.verify();
}

#[ignore]
#[test]
fn add_non_provider_funds() {
    // First element of tuple is the delta the second element is the total after the delta change
    let test_cases = vec![(10, 10), (20, 30), (40, 70)];

    let client_addr = Address::new_id(CLIENT_ID);
    let worker_addr = Address::new_id(WORKER_ID);

    for caller_addr in vec![client_addr, worker_addr] {
        let mut rt = setup();

        for test_case in test_cases.clone() {
            rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), caller_addr);

            let amount = TokenAmount::from(test_case.0 as u64);
            rt.set_value(amount);
            rt.expect_validate_caller_type(CALLER_TYPES_SIGNABLE.to_vec());

            assert!(rt
                .call(
                    &MARKET_ACTOR_CODE_ID.clone(),
                    Method::AddBalance as u64,
                    &Serialized::serialize(caller_addr.clone()).unwrap(),
                )
                .is_ok());

            rt.verify();

            assert_eq!(
                get_escrow_balance(&rt, &caller_addr).unwrap(),
                TokenAmount::from(test_case.1 as u8)
            );
        }
    }
}

#[ignore]
#[test]
fn withdraw_provider_to_owner() {
    let mut rt = setup();

    let owner_addr = Address::new_id(OWNER_ID);
    let worker_addr = Address::new_id(WORKER_ID);
    let provider_addr = Address::new_id(PROVIDER_ID);

    let amount = TokenAmount::from(20u8);
    add_provider_funds(
        &mut rt,
        provider_addr.clone(),
        owner_addr.clone(),
        worker_addr.clone(),
        amount.clone(),
    );

    assert_eq!(amount, get_escrow_balance(&rt, &provider_addr).unwrap());

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

    assert!(rt
        .call(
            &MARKET_ACTOR_CODE_ID.clone(),
            Method::WithdrawBalance as u64,
            &Serialized::serialize(params).unwrap(),
        )
        .is_ok());

    rt.verify();

    assert_eq!(
        get_escrow_balance(&rt, &provider_addr).unwrap(),
        TokenAmount::from(19u8)
    );
}

#[ignore]
#[test]
fn withdraw_non_provider() {
    // Test is currently failing because curr_epoch  is 0. When subtracted by 1, it goe snmegative causing a overflow error
    let mut rt = setup();

    let client_addr = Address::new_id(CLIENT_ID);

    let amount = TokenAmount::from(20u8);
    add_participant_funds(&mut rt, client_addr.clone(), amount.clone());

    assert_eq!(amount, get_escrow_balance(&rt, &client_addr).unwrap());

    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), client_addr.clone());
    rt.expect_validate_caller_type(vec![
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

    assert!(rt
        .call(
            &MARKET_ACTOR_CODE_ID.clone(),
            Method::WithdrawBalance as u64,
            &Serialized::serialize(params).unwrap(),
        )
        .is_ok());

    rt.verify();

    assert_eq!(
        get_escrow_balance(&rt, &client_addr).unwrap(),
        TokenAmount::from(19u8)
    );
}

#[ignore]
#[test]
fn client_withdraw_more_than_available() {
    let mut rt = setup();

    let client_addr = Address::new_id(CLIENT_ID);

    let amount = TokenAmount::from(20u8);
    add_participant_funds(&mut rt, client_addr.clone(), amount.clone());

    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), client_addr.clone());
    rt.expect_validate_caller_type(vec![
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

    assert!(rt
        .call(
            &MARKET_ACTOR_CODE_ID.clone(),
            Method::WithdrawBalance as u64,
            &Serialized::serialize(params).unwrap(),
        )
        .is_ok());

    rt.verify();

    assert_eq!(
        get_escrow_balance(&rt, &client_addr).unwrap(),
        TokenAmount::from(0u8)
    );
}

#[ignore]
#[test]
fn worker_withdraw_more_than_available() {
    let mut rt = setup();

    let owner_addr = Address::new_id(OWNER_ID);
    let worker_addr = Address::new_id(WORKER_ID);
    let provider_addr = Address::new_id(PROVIDER_ID);

    let amount = TokenAmount::from(20u8);
    add_provider_funds(
        &mut rt,
        provider_addr.clone(),
        owner_addr.clone(),
        worker_addr.clone(),
        amount.clone(),
    );

    assert_eq!(amount, get_escrow_balance(&rt, &provider_addr).unwrap());

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

    assert!(rt
        .call(
            &MARKET_ACTOR_CODE_ID.clone(),
            Method::WithdrawBalance as u64,
            &Serialized::serialize(params).unwrap(),
        )
        .is_ok());

    rt.verify();

    assert_eq!(
        get_escrow_balance(&rt, &provider_addr).unwrap(),
        TokenAmount::from(0u8)
    );
}

fn expect_provider_control_address(
    rt: &mut MockRuntime,
    provider: Address,
    owner: Address,
    worker: Address,
) {
    rt.expect_validate_caller_addr(vec![owner.clone(), worker.clone()]);

    let return_value = GetControlAddressesReturn {
        owner: owner.clone(),
        worker: worker.clone(),
        control_addresses: Vec::new(),
    };

    rt.expect_send(
        provider.clone(),
        MinerMethod::ControlAddresses as u64,
        Serialized::default(),
        TokenAmount::from(0u8),
        Serialized::serialize(return_value).unwrap(),
        ExitCode::Ok,
    );
}

fn add_provider_funds(
    rt: &mut MockRuntime,
    provider: Address,
    owner: Address,
    worker: Address,
    amount: TokenAmount,
) {
    rt.set_value(amount.clone());

    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), owner.clone());
    expect_provider_control_address(rt, provider, owner, worker);

    assert!(rt
        .call(
            &MARKET_ACTOR_CODE_ID.clone(),
            Method::AddBalance as u64,
            &Serialized::serialize(provider.clone()).unwrap(),
        )
        .is_ok());

    rt.verify();

    rt.balance = rt.balance.clone() + amount;
}

fn add_participant_funds(rt: &mut MockRuntime, addr: Address, amount: TokenAmount) {
    rt.set_value(amount.clone());

    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), addr.clone());

    rt.expect_validate_caller_type(vec![
        ACCOUNT_ACTOR_CODE_ID.clone(),
        MULTISIG_ACTOR_CODE_ID.clone(),
    ]);

    assert!(rt
        .call(
            &MARKET_ACTOR_CODE_ID.clone(),
            Method::AddBalance as u64,
            &Serialized::serialize(addr.clone()).unwrap(),
        )
        .is_ok());

    rt.verify();

    rt.balance = rt.balance.clone() + amount;
}

fn construct_and_verify(rt: &mut MockRuntime) {
    rt.expect_validate_caller_addr(vec![SYSTEM_ACTOR_ADDR.clone()]);
    assert_eq!(
        Serialized::default(),
        rt.call(
            &*MARKET_ACTOR_CODE_ID,
            METHOD_CONSTRUCTOR,
            &Serialized::default(),
        )
        .unwrap()
    );
    rt.verify();
}
