// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod common;

use actor::{account::State, ACCOUNT_ACTOR_CODE_ID, SYSTEM_ACTOR_ADDR, SYSTEM_ACTOR_CODE_ID};
use address::Address;
use common::*;
use db::MemoryDB;
use vm::{ExitCode, Serialized};

macro_rules! account_tests {
    ($($name:ident: $value:expr,)*) => {
        $(
            #[test]
            fn $name() {
                let (addr, exit_code) = $value;

                let bs = MemoryDB::default();
                let receiver = Address::new_id(100);
                let mut rt = MockRuntime::new(&bs, receiver.clone());
                rt.caller = *SYSTEM_ACTOR_ADDR;
                rt.caller_type = SYSTEM_ACTOR_CODE_ID.clone();
                rt.expect_validate_caller_addr(&[*SYSTEM_ACTOR_ADDR]);

                if exit_code.is_success() {
                    rt
                    .call(
                        &*ACCOUNT_ACTOR_CODE_ID,
                        1,
                        &Serialized::serialize(addr).unwrap(),
                    )
                    .unwrap();

                    let state: State = rt.get_state().unwrap();

                    assert_eq!(state.address, addr);
                    rt.expect_validate_caller_any();

                    let pk: Address = rt
                        .call(&*ACCOUNT_ACTOR_CODE_ID, 2, &Serialized::default())
                        .unwrap()
                        .deserialize()
                        .unwrap();
                    assert_eq!(pk, addr);
                } else {
                    let res = rt.call(
                        &*ACCOUNT_ACTOR_CODE_ID,
                        1,
                        &Serialized::serialize(addr).unwrap(),
                    ).map_err(|e| e.exit_code());
                    assert_eq!(res, Err(exit_code))
                }
                rt.verify();
            }
        )*
    }
}

account_tests! {
    happy_construct_secp256k1_address: (
        Address::new_secp256k1(&[1, 2, 3]),
        ExitCode::Ok
    ),
    happy_construct_bls_address: (
        Address::new_bls(&[1; address::BLS_PUB_LEN]).unwrap(),
        ExitCode::Ok
    ),
    fail_construct_id_address: (
        Address::new_id(1),
        ExitCode::ErrIllegalArgument
    ),
    fail_construct_actor_address: (
        Address::new_actor(&[1, 2, 3]),
        ExitCode::ErrIllegalArgument
    ),
}
