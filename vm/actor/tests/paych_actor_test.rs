// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod common;

use actor::{
    paych::{ConstructorParams, Method, SignedVoucher, State as PState, UpdateChannelStateParams},
    ACCOUNT_ACTOR_CODE_ID, INIT_ACTOR_ADDR, INIT_ACTOR_CODE_ID, MULTISIG_ACTOR_CODE_ID,
    PAYCH_ACTOR_CODE_ID, REWARD_ACTOR_ADDR, REWARD_ACTOR_CODE_ID, SYSTEM_ACTOR_ADDR,
    SYSTEM_ACTOR_CODE_ID,
};
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use common::*;
use crypto::Signature;
use db::MemoryDB;
use derive_builder::Builder;
use ipld_blockstore::BlockStore;
use message::UnsignedMessage;
use num_bigint::BigInt;
use vm::{ExitCode, Serialized, TokenAmount, METHOD_CONSTRUCTOR};

#[test]
fn create_paych_actor_test() {
    let paych_addr = Address::new_id(100);
    let payer_addr = Address::new_id(101);
    let caller_addr = Address::new_id(102);

    let bs = MemoryDB::default();
    let message = UnsignedMessage::builder()
        .from(*SYSTEM_ACTOR_ADDR)
        .to(paych_addr)
        .build()
        .unwrap();
    let mut rt = MockRuntime::new(&bs, message);
    rt.set_caller(INIT_ACTOR_CODE_ID.clone(), caller_addr);
    rt.actor_code_cids
        .insert(payer_addr, ACCOUNT_ACTOR_CODE_ID.clone());
    rt.actor_code_cids
        .insert(caller_addr, ACCOUNT_ACTOR_CODE_ID.clone());
    construct_and_verify(&mut rt, payer_addr, caller_addr);
}

#[test]
fn actor_doesnt_exist_test() {
    let paych_addr = Address::new_id(100);
    let payer_addr = Address::new_id(101);
    let caller_addr = Address::new_id(102);
    let bs = MemoryDB::default();
    let message = UnsignedMessage::builder()
        .from(*SYSTEM_ACTOR_ADDR)
        .to(paych_addr)
        .build()
        .unwrap();
    let mut rt = MockRuntime::new(&bs, message);
    rt.set_caller(INIT_ACTOR_CODE_ID.clone(), caller_addr);
    rt.actor_code_cids
        .insert(payer_addr, ACCOUNT_ACTOR_CODE_ID.clone());
    rt.expect_validate_caller_type(&[INIT_ACTOR_CODE_ID.clone()]);
    let params = ConstructorParams {
        to: paych_addr,
        from: payer_addr,
    };

    let error = rt
        .call(
            &PAYCH_ACTOR_CODE_ID.clone(),
            METHOD_CONSTRUCTOR,
            &Serialized::serialize(params).unwrap(),
        )
        .unwrap_err();
    assert_eq!(error.exit_code(), ExitCode::ErrIllegalArgument);
}

#[test]
fn actor_constructor_fails() {
    let paych_addr = Address::new_id(100);
    let payer_addr = Address::new_id(101);
    let caller_addr = Address::new_id(102);

    struct TestCase {
        paych_addr: Address,
        caller_code: Cid,
        new_actor_code: Cid,
        payer_code: Cid,
        expected_exit_code: ExitCode,
    }

    let test_cases: Vec<TestCase> = vec![TestCase {
        paych_addr: paych_addr,
        caller_code: INIT_ACTOR_CODE_ID.clone(),
        new_actor_code: MULTISIG_ACTOR_CODE_ID.clone(),
        payer_code: ACCOUNT_ACTOR_CODE_ID.clone(),
        expected_exit_code: ExitCode::ErrIllegalArgument,
    }];

    for test_case in test_cases {
        let bs = MemoryDB::default();
        let message = UnsignedMessage::builder()
            .from(*SYSTEM_ACTOR_ADDR)
            .to(paych_addr)
            .build()
            .unwrap();
        let mut rt = MockRuntime::new(&bs, message);
        rt.set_caller(test_case.caller_code, caller_addr);
        rt.actor_code_cids
            .insert(test_case.paych_addr, test_case.new_actor_code);
        rt.actor_code_cids.insert(payer_addr, test_case.payer_code);
        rt.expect_validate_caller_type(&[INIT_ACTOR_CODE_ID.clone()]);
        let params = ConstructorParams {
            to: test_case.paych_addr,
            from: Address::new_id(10001),
        };
        let error = rt
            .call(
                &PAYCH_ACTOR_CODE_ID.clone(),
                METHOD_CONSTRUCTOR,
                &Serialized::serialize(params).unwrap(),
            )
            .unwrap_err();
        assert_eq!(test_case.expected_exit_code, error.exit_code());
    }
}

mod create_lane_tests {
    use super::*;
    #[derive(Builder)]
    #[builder(name = "TestCaseBuilder")]
    struct TestCase {
        #[builder(default = "ACCOUNT_ACTOR_CODE_ID.clone()")]
        target_code: Cid,
        #[builder(default)]
        balance: u64,
        #[builder(default)]
        recieved: u64,
        #[builder(default = "1")]
        epoch: ChainEpoch,
        #[builder(default = "1")]
        tl_min: ChainEpoch,
        #[builder(default = "0")]
        tl_max: ChainEpoch,
        #[builder(default)]
        lane: u64,
        #[builder(default)]
        nonce: u64,
        #[builder(default = "1")]
        amt: i64,
        #[builder(default)]
        secret_preimage: Vec<u8>,
        #[builder(default)]
        sig: Option<Signature>,
        #[builder(default = "true")]
        verify_sig: bool,
        #[builder(default = "ExitCode::ErrIllegalArgument")]
        exp_exit_code: ExitCode,
    }
    impl TestCase {
        pub fn builder() -> TestCaseBuilder {
            TestCaseBuilder::default()
        }
    }

    #[test]
    fn create_lane_test() {
        let init_actor_addr = Address::new_id(100);
        let paych_addr = Address::new_id(101);
        let payer_addr = Address::new_id(102);
        let payee_addr = Address::new_id(103);
        let paych_balance = TokenAmount::from(9 as u64);
        let sig = Option::Some(Signature::new_bls("doesn't matter".as_bytes().to_vec()));

        let test_cases: Vec<TestCase> = vec![
            // TestCase::builder()
            //     .sig(sig.clone())
            //     .exp_exit_code(ExitCode::Ok)
            //     .build()
            //     .unwrap(),
            TestCase::builder()
                .amt(-1)
                .sig(sig.clone())
                .exp_exit_code(ExitCode::ErrIllegalState)
                .build()
                .unwrap(),
            TestCase::builder()
                .amt(10)
                .sig(sig.clone())
                .exp_exit_code(ExitCode::ErrIllegalState)
                .build()
                .unwrap(),
            TestCase::builder().sig(Option::None).build().unwrap(),
            TestCase::builder()
                .tl_min(10)
                .sig(sig.clone())
                .build()
                .unwrap(),
            TestCase::builder()
                .epoch(10)
                .tl_max(5)
                .sig(sig.clone())
                .build()
                .unwrap(),
            TestCase::builder()
                .sig(sig.clone())
                .verify_sig(false)
                .build()
                .unwrap(),
            TestCase::builder()
                .sig(sig.clone())
                .secret_preimage(vec![0; 2 << 21])
                .exp_exit_code(ExitCode::ErrIllegalState)
                .build()
                .unwrap(),
        ];

        for test_case in test_cases {
            let bs = MemoryDB::default();
            let message = UnsignedMessage::builder()
                .from(*SYSTEM_ACTOR_ADDR)
                .to(paych_addr)
                .build()
                .unwrap();
            let mut rt = MockRuntime::new(&bs, message);
            rt.epoch = test_case.epoch;
            rt.balance = TokenAmount::from(test_case.balance);
            rt.set_caller(INIT_ACTOR_CODE_ID.clone(), *INIT_ACTOR_ADDR);
            rt.actor_code_cids
                .insert(payee_addr, ACCOUNT_ACTOR_CODE_ID.clone());
            rt.actor_code_cids
                .insert(payer_addr, ACCOUNT_ACTOR_CODE_ID.clone());
            construct_and_verify(&mut rt, payer_addr, payee_addr);

            let sv = SignedVoucher {
                time_lock_min: test_case.tl_min,
                time_lock_max: test_case.tl_max,
                secret_pre_image: test_case.secret_preimage.clone(),
                extra: Option::None,
                lane: test_case.lane,
                nonce: test_case.nonce,
                amount: BigInt::from(test_case.amt),
                min_settle_height: 0,
                merges: vec![],
                signature: test_case.sig.clone(),
            };

            let ucp = UpdateChannelStateParams {
                sv: sv,
                secret: vec![],
                proof: vec![],
            };

            rt.set_caller(test_case.target_code, payee_addr);
            rt.expect_validate_caller_addr(&[payer_addr, payee_addr]);
            if test_case.exp_exit_code == ExitCode::Ok {
                assert!(rt
                    .call(
                        &PAYCH_ACTOR_CODE_ID.clone(),
                        Method::UpdateChannelState as u64,
                        &Serialized::serialize(ucp).unwrap()
                    )
                    .is_ok());
                let st: PState = rt.get_state().unwrap();
                assert_eq!(st.lane_states.len(), 1);
                let ls = st.lane_states.first().unwrap();
                let sv = SignedVoucher {
                    time_lock_min: test_case.tl_min,
                    time_lock_max: test_case.tl_max,
                    secret_pre_image: test_case.secret_preimage,
                    extra: Option::None,
                    lane: test_case.lane,
                    nonce: test_case.nonce,
                    amount: BigInt::from(test_case.amt),
                    min_settle_height: 0,
                    merges: vec![],
                    signature: test_case.sig,
                };
                assert_eq!(sv.amount, ls.redeemed);
                assert_eq!(sv.nonce, ls.nonce);
                assert_eq!(sv.lane, ls.id);
            } else {
                let error = rt
                    .call(
                        &PAYCH_ACTOR_CODE_ID.clone(),
                        Method::UpdateChannelState as u64,
                        &Serialized::serialize(ucp).unwrap(),
                    )
                    .unwrap_err();
                assert_eq!(error.exit_code(), test_case.exp_exit_code);
                verify_initial_state(&mut rt, payer_addr, payee_addr);
            }
            rt.verify();
        }
    }
}

fn construct_and_verify<BS: BlockStore>(
    rt: &mut MockRuntime<'_, BS>,
    sender: Address,
    receiver: Address,
) {
    let params = ConstructorParams {
        from: sender,
        to: receiver,
    };
    rt.expect_validate_caller_type(&[INIT_ACTOR_CODE_ID.clone()]);
    let v = rt
        .call(
            &PAYCH_ACTOR_CODE_ID,
            METHOD_CONSTRUCTOR,
            &Serialized::serialize(&params).unwrap(),
        )
        .unwrap();
    rt.verify();
    verify_initial_state(rt, sender, receiver);
}

fn verify_initial_state<BS: BlockStore>(
    rt: &mut MockRuntime<'_, BS>,
    sender: Address,
    receiver: Address,
) {
    let state: PState = rt.get_state().unwrap();
    let expected_state = PState::new(sender, receiver);
    assert_eq!(expected_state.to, state.to);
    assert_eq!(expected_state.from, state.from);
    assert_eq!(expected_state.min_settle_height, state.min_settle_height);
    assert_eq!(expected_state.settling_at, state.settling_at);
    assert_eq!(expected_state.to_send, state.to_send);
    assert_eq!(state.lane_states.len(), 0);
}
