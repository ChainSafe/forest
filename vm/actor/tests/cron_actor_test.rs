use actor::{
    cron::{ConstructorParams, Entry},
    CRON_ACTOR_CODE_ID, SYSTEM_ACTOR_ADDR, SYSTEM_ACTOR_CODE_ID,
};
// use crate::tests::mock_rt::*;
use address::Address;
use db::MemoryDB;
use vm::Serialized;
#[path = "mock_rt.rs"]
mod mock_rt;
use ipld_blockstore::BlockStore;
use mock_rt::*;
use vm::ExitCode;

#[test]
fn epoch_tick_with_empty_entries() {
    let bs = MemoryDB::default();
    let receiver = Address::new_id(100).unwrap();
    let mut rt = MockRuntime::new(&bs, receiver.clone());
    rt.caller = SYSTEM_ACTOR_ADDR.clone();
    rt.caller_type = SYSTEM_ACTOR_CODE_ID.clone();

    construct_and_verify(&mut rt, ConstructorParams { entries: vec![] });
    epoch_tick_and_verify(&mut rt);
}
#[test]
fn epoch_tick_with_entries() {
    let bs = MemoryDB::default();
    let receiver = Address::new_id(100).unwrap();
    let mut rt = MockRuntime::new(&bs, receiver.clone());
    rt.caller = SYSTEM_ACTOR_ADDR.clone();
    rt.caller_type = SYSTEM_ACTOR_CODE_ID.clone();

    let entry1 = Entry {
        receiver: Address::new_id(1001).unwrap(),
        method_num: 1001,
    };
    let entry2 = Entry {
        receiver: Address::new_id(1002).unwrap(),
        method_num: 1002,
    };
    let entry3 = Entry {
        receiver: Address::new_id(1003).unwrap(),
        method_num: 1003,
    };
    let entry4 = Entry {
        receiver: Address::new_id(1004).unwrap(),
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

    construct_and_verify(&mut rt, params);

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

fn construct_and_verify<BS: BlockStore>(rt: &mut MockRuntime<'_, BS>, params: ConstructorParams) {
    rt.expect_validate_caller_addr(&[SYSTEM_ACTOR_ADDR.clone()]);
    let ret = rt
        .call(
            &*CRON_ACTOR_CODE_ID,
            1,
            &Serialized::serialize(params).unwrap(),
        )
        .unwrap();
    assert_eq!(Serialized::default(), ret);
    rt.verify();
}

fn epoch_tick_and_verify<BS: BlockStore>(rt: &mut MockRuntime<'_, BS>) {
    rt.expect_validate_caller_addr(&[SYSTEM_ACTOR_ADDR.clone()]);
    let ret = rt
        .call(&*CRON_ACTOR_CODE_ID, 2, &Serialized::default())
        .unwrap();
    assert_eq!(Serialized::default(), ret);
    rt.verify();
}
