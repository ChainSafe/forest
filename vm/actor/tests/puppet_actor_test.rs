// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod common;

use actor::{puppet::*, PUPPET_ACTOR_CODE_ID, SYSTEM_ACTOR_ADDR};
use address::Address;
use common::*;
use vm::{ActorError, ExitCode, Serialized, TokenAmount, METHOD_CONSTRUCTOR};

fn setup() -> MockRuntime {
    let receiver = Address::new_id(100);

    let mut rt = MockRuntime {
        receiver,
        caller: *SYSTEM_ACTOR_ADDR,
        ..Default::default()
    };
    construct_and_verify(&mut rt);
    rt
}

fn construct_and_verify(rt: &mut MockRuntime) {
    rt.expect_validate_caller_any();
    assert!(rt
        .call(
            &*PUPPET_ACTOR_CODE_ID,
            METHOD_CONSTRUCTOR,
            &Serialized::default()
        )
        .is_ok());
    rt.verify();
}

fn puppet_send(rt: &mut MockRuntime, params: SendParams) -> Result<SendReturn, ActorError> {
    rt.expect_validate_caller_any();
    let serialized = rt.call(
        &*PUPPET_ACTOR_CODE_ID,
        Method::Send as u64,
        &Serialized::serialize(params).unwrap(),
    )?;

    rt.verify();
    Ok(Serialized::deserialize(&serialized)?)
}

#[test]
fn simple_send() {
    let mut rt = setup();
    let to = Address::new_id(101);
    let amount = TokenAmount::from(100);
    let params = Serialized::serialize(vec![1, 2, 3, 4, 5]).unwrap();
    let send_params = SendParams {
        to: to,
        value: amount.clone(),
        method: Method::Constructor as u64,
        params: params.clone(),
    };

    rt.balance = amount.clone();
    let exp_ret = Serialized::serialize(vec![6, 7, 8, 9, 10]).unwrap();
    rt.expect_send(
        to,
        METHOD_CONSTRUCTOR,
        params,
        amount,
        exp_ret.clone(),
        ExitCode::Ok,
    );

    let ret = puppet_send(&mut rt, send_params);

    assert!(ret.is_ok());

    assert_eq!(Some(exp_ret), ret.unwrap().return_bytes);
}

#[test]
fn serialize_test() {
    let mut v: Vec<FailToMarshalCBOR> = vec![];

    // Should pass becuase vec is empty
    assert!(Serialized::serialize(&v).is_ok());

    v.push(FailToMarshalCBOR::default());

    // Should fail becuase vec is no longer empty
    assert!(Serialized::serialize(v).is_err());

    let mut v: Vec<Option<FailToMarshalCBOR>> = vec![];

    v.push(Some(FailToMarshalCBOR::default()));

    // SHould only fail if a actual instance of FailToMarshalCBOR is used
    assert!(Serialized::serialize(v).is_err());
}
