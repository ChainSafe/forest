// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod common;
use actor::{
    cron::{ConstructorParams, Entry, State},
    CRON_ACTOR_CODE_ID, SYSTEM_ACTOR_ADDR, SYSTEM_ACTOR_CODE_ID,
};
use address::Address;
use common::*;
use db::MemoryDB;
use ipld_blockstore::BlockStore;
use message::UnsignedMessage;
use vm::{ExitCode, Serialized};

fn construct_runtime<'a, BS: BlockStore>(bs: &'a BS) -> MockRuntime<'a, BS> {
    let receiver = Address::new_id(100);

    let message = UnsignedMessage::builder()
        .from(SYSTEM_ACTOR_ADDR.clone())
        .to(receiver.clone())
        .build()
        .unwrap();
    let mut rt = MockRuntime::new(bs, message);
    rt.caller_type = SYSTEM_ACTOR_CODE_ID.clone();
    return rt;
}
#[test]
fn construct_with_empty_entries() {
    let bs = MemoryDB::default();

    let mut rt = construct_runtime(&bs);

    construct_and_verify(&mut rt, &ConstructorParams { entries: vec![] });
    let state: State = rt.get_state().unwrap();

    assert_eq!(state.entries, vec![]);
}

#[test]
fn construct_with_entries() {
    let bs = MemoryDB::default();

    let mut rt = construct_runtime(&bs);

    let entry1 = Entry {
        receiver: Address::new_id(1001),
        method_num: 1001,
    };
    let entry2 = Entry {
        receiver: Address::new_id(1002),
        method_num: 1002,
    };
    let entry3 = Entry {
        receiver: Address::new_id(1003),
        method_num: 1003,
    };
    let entry4 = Entry {
        receiver: Address::new_id(1004),
        method_num: 1004,
    };

    let params = ConstructorParams {
        entries: vec![entry1, entry2, entry3, entry4],
    };

    construct_and_verify(&mut rt, &params);

    let state: State = rt.get_state().unwrap();

    assert_eq!(state.entries, params.entries);
}

#[test]
fn epoch_tick_with_empty_entries() {
    let bs = MemoryDB::default();
    let mut rt = construct_runtime(&bs);

    construct_and_verify(&mut rt, &ConstructorParams { entries: vec![] });
    epoch_tick_and_verify(&mut rt);
}
#[test]
fn epoch_tick_with_entries() {
    let bs = MemoryDB::default();
    let mut rt = construct_runtime(&bs);

    let entry1 = Entry {
        receiver: Address::new_id(1001),
        method_num: 1001,
    };
    let entry2 = Entry {
        receiver: Address::new_id(1002),
        method_num: 1002,
    };
    let entry3 = Entry {
        receiver: Address::new_id(1003),
        method_num: 1003,
    };
    let entry4 = Entry {
        receiver: Address::new_id(1004),
        method_num: 1004,
    };

    let params = ConstructorParams {
        entries: vec![
            entry1.clone(),
            entry2.clone(),
            entry3.clone(),
            entry4.clone(),
        ],
    };

    construct_and_verify(&mut rt, &params);

    // ExitCodes dont matter here
    rt.expect_send(
        entry1.receiver.clone(),
        entry1.method_num,
        Serialized::default(),
        0u8.into(),
        Serialized::default(),
        ExitCode::Ok,
    );
    rt.expect_send(
        entry2.receiver.clone(),
        entry2.method_num,
        Serialized::default(),
        0u8.into(),
        Serialized::default(),
        ExitCode::ErrIllegalArgument,
    );
    rt.expect_send(
        entry3.receiver.clone(),
        entry3.method_num,
        Serialized::default(),
        0u8.into(),
        Serialized::default(),
        ExitCode::Ok,
    );
    rt.expect_send(
        entry4.receiver.clone(),
        entry4.method_num,
        Serialized::default(),
        0u8.into(),
        Serialized::default(),
        ExitCode::Ok,
    );

    epoch_tick_and_verify(&mut rt);
}

fn construct_and_verify<BS: BlockStore>(rt: &mut MockRuntime<'_, BS>, params: &ConstructorParams) {
    rt.expect_validate_caller_addr(&[*SYSTEM_ACTOR_ADDR]);
    let ret = rt
        .call(
            &*CRON_ACTOR_CODE_ID,
            1,
            &Serialized::serialize(&params).unwrap(),
        )
        .unwrap();
    assert_eq!(Serialized::default(), ret);
    rt.verify();
}

fn epoch_tick_and_verify<BS: BlockStore>(rt: &mut MockRuntime<'_, BS>) {
    rt.expect_validate_caller_addr(&[*SYSTEM_ACTOR_ADDR]);
    let ret = rt
        .call(&*CRON_ACTOR_CODE_ID, 2, &Serialized::default())
        .unwrap();
    assert_eq!(Serialized::default(), ret);
    rt.verify();
}
