// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod common;

use actor::{
    paych::{
        ConstructorParams, LaneState, Merge, Method, ModVerifyParams, PaymentVerifyParams,
        SignedVoucher, State as PState, UpdateChannelStateParams, LANE_LIMIT, SETTLE_DELAY,
    },
    ACCOUNT_ACTOR_CODE_ID, INIT_ACTOR_ADDR, INIT_ACTOR_CODE_ID, MULTISIG_ACTOR_CODE_ID,
    PAYCH_ACTOR_CODE_ID,  SYSTEM_ACTOR_ADDR
};
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use common::*;
use crypto::Signature;
use db::MemoryDB;
use derive_builder::Builder;
use encoding::to_vec;
use ipld_blockstore::BlockStore;
use message::UnsignedMessage;
use num_bigint::{BigInt, Sign};
use vm::{ExitCode, Serialized, TokenAmount, METHOD_CONSTRUCTOR, METHOD_SEND};

struct LaneParams {
    epoch_num: ChainEpoch,
    from: Address,
    to: Address,
    amt: i64,
    lane: u64,
    nonce: u64,
}

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
    #[derive(Builder, Debug)]
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
            TestCase::builder()
                .sig(sig.clone())
                .exp_exit_code(ExitCode::Ok)
                .build()
                .unwrap(),
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
                .build()
                .unwrap(),
        ];

        for test_case in test_cases {
            println!("Executing the next test case ");
            let bs = MemoryDB::default();
            let message = UnsignedMessage::builder()
                .from(*SYSTEM_ACTOR_ADDR)
                .to(paych_addr)
                .gas_limit(1000)
                .build()
                .unwrap();
            let mut rt = MockRuntime::new(&bs, message);
            rt.epoch = test_case.epoch;
            rt.balance = TokenAmount::from(paych_balance.clone());
            rt.set_caller(INIT_ACTOR_CODE_ID.clone(), init_actor_addr);

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
                sv: sv.clone(),
                secret: vec![],
                proof: vec![],
            };

            rt.set_caller(test_case.target_code, payee_addr);
            rt.expect_validate_caller_addr(&[payer_addr, payee_addr]);

            if test_case.sig.is_some() && test_case.secret_preimage.len() == 0 {
                if !test_case.verify_sig {
                    rt.expect_verify_signature(
                        sv.clone().signature.unwrap(),
                        payer_addr,
                        to_vec(&sv).unwrap(),
                        ExitCode::ErrIllegalState,
                    );
                } else {
                    rt.expect_verify_signature(
                        sv.clone().signature.unwrap(),
                        payer_addr,
                        to_vec(&sv).unwrap(),
                        ExitCode::Ok,
                    );
                }
            }

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
                println!("error message is {}", error.msg());
                assert_eq!(error.exit_code(), test_case.exp_exit_code);
                verify_initial_state(&mut rt, payer_addr, payee_addr);
            }
            rt.verify();
        }
    }
}

#[test]
fn redeem_voucher_one_lane() {
    let bs = MemoryDB::default();
    let (mut rt, mut sv) = require_create_cannel_with_lanes(&bs, 1);
    let state: PState = rt.get_state().unwrap();
    let new_voucher_amount = BigInt::from(9);
    sv.amount = new_voucher_amount;
    let ucp = UpdateChannelStateParams {
        proof: vec![],
        secret: vec![],
        sv: sv.clone(),
    };
    let payee_addr = Address::new_id(103);
    let payer_addr = Address::new_id(102);
    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), payee_addr);
    rt.expect_validate_caller_addr(&[state.from, state.to]);
    let s = sv.signature.clone().unwrap();

    rt.expect_verify_signature(s.clone(), payer_addr, to_vec(&sv).unwrap(), ExitCode::Ok);

    assert!(rt
        .call(
            &PAYCH_ACTOR_CODE_ID.clone(),
            Method::UpdateChannelState as u64,
            &Serialized::serialize(ucp).unwrap(),
        )
        .is_ok());

    rt.verify();
    let exp_ls = LaneState {
        id: 0,
        redeemed: BigInt::from(9),
        nonce: 1,
    };
    let exp_state = PState {
        from: state.from,
        to: state.to,
        to_send: TokenAmount::from(9 as u64),
        settling_at: state.settling_at,
        min_settle_height: state.min_settle_height,
        lane_states: vec![exp_ls],
    };
    verify_state(&mut rt, 1, exp_state);
}

#[test]
fn redeem_voucher_correct_lane() {
    let bs = MemoryDB::default();
    let (mut rt, mut sv) = require_create_cannel_with_lanes(&bs, 3);
    let state: PState = rt.get_state().unwrap();
    let initial_amount = state.to_send;
    sv.amount = BigInt::from(9);
    sv.lane = 1;
    let ls_to_update: &LaneState = &state.lane_states[1];
    sv.nonce = ls_to_update.nonce + 1;
    let payee_addr = Address::new_id(103);
    let payer_addr = Address::new_id(102);

    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), payee_addr);
    rt.expect_validate_caller_addr(&[state.from, state.to]);
    rt.expect_verify_signature(
        sv.clone().signature.unwrap(),
        payer_addr,
        to_vec(&sv).unwrap(),
        ExitCode::Ok,
    );

    let ucp = UpdateChannelStateParams {
        proof: vec![],
        secret: vec![],
        sv: sv.clone(),
    };

    assert!(rt
        .call(
            &PAYCH_ACTOR_CODE_ID.clone(),
            Method::UpdateChannelState as u64,
            &Serialized::serialize(ucp).unwrap()
        )
        .is_ok());
    rt.verify();

    let state: PState = rt.get_state().unwrap();
    let ls_updated: &LaneState = &state.lane_states[1];
    let big_delta =
        &sv.amount - BigInt::from_signed_bytes_be(&ls_to_update.redeemed.to_signed_bytes_be());

    let exp_send = big_delta + BigInt::from_signed_bytes_be(&initial_amount.to_radix_be(10));
    assert_eq!(
        exp_send,
        BigInt::from_radix_be(Sign::Plus, &state.to_send.to_radix_be(10), 10).unwrap()
    );
    assert_eq!(sv.amount, ls_updated.redeemed);
    assert_eq!(sv.nonce, ls_updated.nonce);
}

#[test]
fn merge_success() {
    let num_lanes = 3;
    let bs = MemoryDB::default();
    let (mut rt, mut sv) = require_create_cannel_with_lanes(&bs, num_lanes);
    let state: PState = rt.get_state().unwrap();
    let state_2: PState = rt.get_state().unwrap();

    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.from);
    let merge_to: &LaneState = &state.lane_states[0];
    let merge_from: &LaneState = &state.lane_states[1];
    sv.lane = merge_to.id;
    let merge_nonce = merge_to.nonce + 10;
    let merges: Vec<Merge> = vec![Merge {
        lane: merge_from.id,
        nonce: merge_nonce,
    }];

    sv.merges = merges;
    let payee_addr = Address::new_id(103);
    rt.expect_validate_caller_addr(&[state.from, state.to]);
    rt.expect_verify_signature(
        sv.clone().signature.unwrap(),
        payee_addr,
        to_vec(&sv).unwrap(),
        ExitCode::Ok,
    );
    let ucp = UpdateChannelStateParams {
        proof: vec![],
        secret: vec![],
        sv: sv.clone(),
    };
    assert!(rt
        .call(
            &PAYCH_ACTOR_CODE_ID.clone(),
            Method::UpdateChannelState as u64,
            &Serialized::serialize(ucp).unwrap()
        )
        .is_ok());
    rt.verify();

    let (sv_amount_sign, sv_amount_bytes) = &sv.amount.to_bytes_be();

    let exp_merge_to = LaneState {
        id: merge_to.id,
        redeemed: BigInt::from_bytes_be(*sv_amount_sign, &sv_amount_bytes),
        nonce: sv.nonce,
    };

    let (from_sign, from_bytes) = &merge_from.redeemed.to_bytes_be();

    let exp_merge_from = LaneState {
        id: merge_from.id,
        redeemed: BigInt::from_bytes_be(*from_sign, &from_bytes),
        nonce: merge_nonce,
    };

    let (to_sign, to_bytes) = &merge_to.redeemed.to_bytes_be();

    let redeemed =
        BigInt::from_bytes_be(*from_sign, &from_bytes) + BigInt::from_bytes_be(*to_sign, &to_bytes);
    let exp_delta = sv.amount - redeemed;
    let exp_send_amt = BigInt::from_bytes_be(Sign::Plus, &state.to_send.to_bytes_be()) + exp_delta;
    let mut exp_state = state_2;
    exp_state.to_send = TokenAmount::from_bytes_be(&exp_send_amt.to_signed_bytes_be());
    exp_state.lane_states = vec![
        exp_merge_to,
        exp_merge_from,
        exp_state.lane_states.pop().unwrap(),
    ];
    verify_state(&mut rt, num_lanes as i64, exp_state);
}

#[test]
fn merge_failue() {
    let lane_vec = vec![1, 1, 1, 0];
    let voucher_vec = vec![10, 0, 10, 10];
    let balance_vec = vec![0, 0, 1, 0];
    let merge_vec = vec![1, 10, 10, 10];
    let exit_vec = vec![
        ExitCode::ErrIllegalArgument,
        ExitCode::ErrIllegalArgument,
        ExitCode::ErrIllegalState,
        ExitCode::ErrIllegalArgument,
    ];
    let num_test_cases = lane_vec.len();
    let payee_addr = Address::new_id(103);

    for i in 0..num_test_cases {
        let bs = MemoryDB::default();
        let (mut rt, mut sv) = require_create_cannel_with_lanes(&bs, 2);
        rt.balance = TokenAmount::from(balance_vec[i] as u64);

        let state: PState = rt.get_state().unwrap();
        let merge_to: &LaneState = &state.lane_states[0];
        let merge_from: &LaneState = &state.lane_states[lane_vec[i]];
        sv.lane = merge_to.id;
        sv.nonce = voucher_vec[i];
        sv.merges = vec![Merge {
            lane: merge_from.id,
            nonce: merge_vec[i],
        }];

        let ucp = UpdateChannelStateParams {
            proof: vec![],
            secret: vec![],
            sv: sv.clone(),
        };

        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.from);
        rt.expect_validate_caller_addr(&[state.from, state.to]);
        rt.expect_verify_signature(
            sv.clone().signature.unwrap(),
            payee_addr,
            to_vec(&sv).unwrap(),
            ExitCode::Ok,
        );
        let v = rt
            .call(
                &PAYCH_ACTOR_CODE_ID.clone(),
                Method::UpdateChannelState as u64,
                &Serialized::serialize(ucp).unwrap(),
            )
            .unwrap_err();
        println!("Message is {:?}", v.msg());
        assert_eq!(v.exit_code(), exit_vec[i]);
        rt.verify();
    }
}

#[test]
fn invalid_merge_lane_999() {
    let bs = MemoryDB::default();
    let (mut rt, mut sv) = require_create_cannel_with_lanes(&bs, 2);
    let state: PState = rt.get_state().unwrap();
    let payee_addr = Address::new_id(103);
    let merge_to: &LaneState = &state.lane_states[0];
    let merge_from = LaneState {
        id: 999,
        nonce: sv.nonce,
        redeemed: BigInt::from(0),
    };

    sv.lane = merge_to.id;
    sv.nonce = 10;
    sv.merges = vec![Merge {
        lane: merge_from.id,
        nonce: sv.nonce,
    }];
    let ucp = UpdateChannelStateParams {
        proof: vec![],
        secret: vec![],
        sv: sv.clone(),
    };

    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.from);
    rt.expect_validate_caller_addr(&[state.from, state.to]);
    rt.expect_verify_signature(
        sv.clone().signature.unwrap(),
        payee_addr,
        to_vec(&sv).unwrap(),
        ExitCode::Ok,
    );
    let v = rt
        .call(
            &PAYCH_ACTOR_CODE_ID.clone(),
            Method::UpdateChannelState as u64,
            &Serialized::serialize(ucp).unwrap(),
        )
        .unwrap_err();
    assert_eq!(v.exit_code(), ExitCode::ErrIllegalArgument);
    rt.verify();
}

#[test]
fn lane_limit_exceeded() {
    let bs = MemoryDB::default();
    let (mut rt, mut sv) = require_create_cannel_with_lanes(&bs, LANE_LIMIT as u64);
    let state: PState = rt.get_state().unwrap();
    let payee_addr = Address::new_id(103);
    sv.lane += 1;
    sv.nonce += 1;
    sv.amount = BigInt::from(100);
    let ucp = UpdateChannelStateParams {
        proof: vec![],
        secret: vec![],
        sv: sv.clone(),
    };

    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.from);
    rt.expect_validate_caller_addr(&[state.from, state.to]);
    rt.expect_verify_signature(
        sv.clone().signature.unwrap(),
        payee_addr,
        to_vec(&sv).unwrap(),
        ExitCode::Ok,
    );
    let v = rt
        .call(
            &PAYCH_ACTOR_CODE_ID.clone(),
            Method::UpdateChannelState as u64,
            &Serialized::serialize(ucp).unwrap(),
        )
        .unwrap_err();
    assert_eq!(v.exit_code(), ExitCode::ErrIllegalArgument);
    rt.verify();
}

#[test]
fn extra_call_succeed() {
    let bs = MemoryDB::default();
    let (mut rt, mut sv) = require_create_cannel_with_lanes(&bs, 1);
    let state: PState = rt.get_state().unwrap();
    let other_addr = Address::new_id(104);
    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.from);
    sv.extra = Some(ModVerifyParams {
        actor: other_addr,
        method: Method::UpdateChannelState as u64,
        data: Serialized::serialize([1, 2, 3, 4]).unwrap(),
    });
    rt.expect_validate_caller_addr(&[state.from, state.to]);
    rt.expect_verify_signature(
        sv.clone().signature.unwrap(),
        state.to,
        to_vec(&sv).unwrap(),
        ExitCode::Ok,
    );
    let exp_send_params = PaymentVerifyParams {
        extra: Serialized::serialize(vec![1, 2, 3, 4]).unwrap(),
        proof: vec![],
    };

    rt.expect_send(
        other_addr,
        Method::UpdateChannelState as u64,
        Serialized::serialize(exp_send_params).unwrap(),
        TokenAmount::from(0u8),
        Serialized::default(),
        ExitCode::Ok,
    );
    let ucp = UpdateChannelStateParams {
        proof: vec![],
        secret: vec![],
        sv: sv.clone(),
    };
    assert!(rt
        .call(
            &PAYCH_ACTOR_CODE_ID.clone(),
            Method::UpdateChannelState as u64,
            &Serialized::serialize(ucp).unwrap()
        )
        .is_ok());
    rt.verify();
}

#[test]
fn extra_call_fail() {
    let bs = MemoryDB::default();
    let (mut rt, mut sv) = require_create_cannel_with_lanes(&bs, 1);
    let state: PState = rt.get_state().unwrap();
    let other_addr = Address::new_id(104);
    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.from);
    sv.extra = Some(ModVerifyParams {
        actor: other_addr,
        method: Method::UpdateChannelState as u64,
        data: Serialized::serialize([1, 2, 3, 4]).unwrap(),
    });
    rt.expect_validate_caller_addr(&[state.from, state.to]);
    let exp_send_params = PaymentVerifyParams {
        extra: Serialized::serialize(vec![1, 2, 3, 4]).unwrap(),
        proof: vec![],
    };
    rt.expect_send(
        other_addr,
        Method::UpdateChannelState as u64,
        Serialized::serialize(exp_send_params).unwrap(),
        TokenAmount::from(0u8),
        Serialized::default(),
        ExitCode::ErrPlaceholder,
    );
    rt.expect_verify_signature(
        sv.clone().signature.unwrap(),
        state.to,
        to_vec(&sv).unwrap(),
        ExitCode::Ok,
    );
    let ucp = UpdateChannelStateParams {
        proof: vec![],
        secret: vec![],
        sv: sv.clone(),
    };
    let v = rt
        .call(
            &PAYCH_ACTOR_CODE_ID.clone(),
            Method::UpdateChannelState as u64,
            &Serialized::serialize(ucp).unwrap(),
        )
        .unwrap_err();
    assert_eq!(v.exit_code(), ExitCode::ErrPlaceholder);
    rt.verify();
}

#[test]
fn update_channel_setting() {
    let bs = MemoryDB::default();
    let (mut rt, sv) = require_create_cannel_with_lanes(&bs, 1);
    rt.epoch = 10;
    let state: PState = rt.get_state().unwrap();
    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.from);
    rt.expect_validate_caller_addr(&[state.from, state.to]);
    assert!(rt
        .call(
            &PAYCH_ACTOR_CODE_ID.clone(),
            Method::Settle as u64,
            &Serialized::default()
        )
        .is_ok());

    let exp_settling_at = SETTLE_DELAY + 10;
    let state: PState = rt.get_state().unwrap();
    assert_eq!(exp_settling_at, state.settling_at);
    assert_eq!(state.min_settle_height, 0);

    let min_settle_vec = vec![0, 2, 12];
    let exp_min_settle_height = vec![state.min_settle_height, 2, 12];
    let exp_settling_at = vec![state.settling_at, state.settling_at, 12];
    let num_test_cases = min_settle_vec.len();

    for i in 0..num_test_cases {
        let mut ucp = UpdateChannelStateParams {
            proof: vec![],
            secret: vec![],
            sv: sv.clone(),
        };
        ucp.sv.min_settle_height = exp_min_settle_height[i];
        rt.expect_validate_caller_addr(&[state.from, state.to]);
        rt.expect_verify_signature(
            ucp.sv.clone().signature.unwrap(),
            state.to,
            to_vec(&ucp.sv).unwrap(),
            ExitCode::Ok,
        );
        assert!(rt
            .call(
                &PAYCH_ACTOR_CODE_ID.clone(),
                Method::UpdateChannelState as u64,
                &Serialized::serialize(ucp).unwrap()
            )
            .is_ok());
        let new_state: PState = rt.get_state().unwrap();
        assert_eq!(exp_settling_at[i], new_state.settling_at);
        assert_eq!(exp_min_settle_height[i], new_state.min_settle_height);
    }
}

#[test]
fn succeed_correct_secret() {
    let bs = MemoryDB::default();
    let (mut rt, sv) = require_create_cannel_with_lanes(&bs, 1);
    let state: PState = rt.get_state().unwrap();
    let ucp = UpdateChannelStateParams {
        proof: vec![],
        secret: vec![],
        sv: sv.clone(),
    };
    rt.expect_validate_caller_addr(&[state.from, state.to]);
    rt.expect_verify_signature(
        ucp.sv.clone().signature.unwrap(),
        state.to,
        to_vec(&sv).unwrap(),
        ExitCode::Ok,
    );
    assert!(rt
        .call(
            &PAYCH_ACTOR_CODE_ID.clone(),
            Method::UpdateChannelState as u64,
            &Serialized::serialize(ucp).unwrap()
        )
        .is_ok());
    rt.verify();
}

#[test]
fn incorrect_secret() {
    let bs = MemoryDB::default();
    let (mut rt, sv) = require_create_cannel_with_lanes(&bs, 1);
    let state: PState = rt.get_state().unwrap();
    let mut ucp = UpdateChannelStateParams {
        proof: vec![],
        secret: b"Profesr".to_vec(),
        sv: sv.clone(),
    };
    let mut mag = b"Magneto".to_vec();
    let mut empty = vec![0; 25];
    mag.append(&mut empty);
    ucp.sv.secret_pre_image = mag;
    rt.expect_validate_caller_addr(&[state.from, state.to]);
    rt.expect_verify_signature(
        ucp.sv.clone().signature.unwrap(),
        state.to,
        to_vec(&ucp.sv).unwrap(),
        ExitCode::Ok,
    );
    let v = rt
        .call(
            &PAYCH_ACTOR_CODE_ID.clone(),
            Method::UpdateChannelState as u64,
            &Serialized::serialize(ucp).unwrap(),
        )
        .unwrap_err();
    assert_eq!(v.exit_code(), ExitCode::ErrIllegalArgument);
    rt.verify();
}

#[test]
fn adjust_settling_at() {
    let bs = MemoryDB::default();
    let (mut rt, _sv) = require_create_cannel_with_lanes(&bs, 1);
    let ep = 10;
    rt.epoch = ep;
    let mut state: PState = rt.get_state().unwrap();
    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.from);
    rt.expect_validate_caller_addr(&[state.from, state.to]);
    assert!(rt
        .call(
            &PAYCH_ACTOR_CODE_ID.clone(),
            Method::Settle as u64,
            &Serialized::default()
        )
        .is_ok());
    let exp_settling_at = ep + SETTLE_DELAY;
    state = rt.get_state().unwrap();
    assert_eq!(state.settling_at, exp_settling_at);
    assert_eq!(state.min_settle_height, 0);
}

#[test]
fn settle_if_height_less() {
    let bs = MemoryDB::default();
    let (mut rt, mut sv) = require_create_cannel_with_lanes(&bs, 1);
    let ep = 10;
    rt.epoch = ep;
    let mut state: PState = rt.get_state().unwrap();
    sv.min_settle_height = (ep + SETTLE_DELAY) + 1;
    let ucp = UpdateChannelStateParams {
        proof: vec![],
        secret: vec![],
        sv: sv.clone(),
    };
    rt.expect_validate_caller_addr(&[state.from, state.to]);
    rt.expect_verify_signature(
        ucp.sv.clone().signature.unwrap(),
        state.to,
        to_vec(&sv).unwrap(),
        ExitCode::Ok,
    );
    assert!(rt
        .call(
            &PAYCH_ACTOR_CODE_ID.clone(),
            Method::UpdateChannelState as u64,
            &Serialized::serialize(&ucp).unwrap()
        )
        .is_ok());
    state = rt.get_state().unwrap();
    assert_eq!(state.settling_at, 0);
    assert_eq!(state.min_settle_height, ucp.sv.min_settle_height);
    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.from);
    rt.expect_validate_caller_addr(&[state.from, state.to]);
    assert!(rt
        .call(
            &PAYCH_ACTOR_CODE_ID.clone(),
            Method::Settle as u64,
            &Serialized::default()
        )
        .is_ok());
    state = rt.get_state().unwrap();
    assert_eq!(state.settling_at, ucp.sv.min_settle_height);
}

#[test]
fn happy_path() {
    let bs = MemoryDB::default();
    let (mut rt, _sv) = require_create_cannel_with_lanes(&bs, 1);
    rt.epoch = 10;
    let mut state: PState = rt.get_state().unwrap();
    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.from);
    rt.expect_validate_caller_addr(&[state.from, state.to]);
    assert!(rt
        .call(
            &PAYCH_ACTOR_CODE_ID.clone(),
            Method::Settle as u64,
            &Serialized::default()
        )
        .is_ok());
    state = rt.get_state().unwrap();
    assert_eq!(state.settling_at, 11);
    rt.expect_validate_caller_addr(&[state.from, state.to]);
    rt.epoch = 12;
    let bal = &rt.balance;
    let sent_to_from = bal - state.to_send.clone();
    rt.expect_send(
        state.from,
        METHOD_SEND,
        Serialized::default(),
        sent_to_from,
        Serialized::default(),
        ExitCode::Ok,
    );
    rt.expect_send(
        state.to,
        METHOD_SEND,
        Serialized::default(),
        state.to_send,
        Serialized::default(),
        ExitCode::Ok,
    );
    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.to);
    rt.expect_validate_caller_addr(&[state.from, state.to]);
    assert!(rt
        .call(
            &PAYCH_ACTOR_CODE_ID.clone(),
            Method::Collect as u64,
            &Serialized::default()
        )
        .is_ok());
    state = rt.get_state().unwrap();
    assert_eq!(state.to_send, TokenAmount::from(0u8));
}

#[test]
fn actor_collect() {
    let dont_settle_vec = vec![true, false, false];
    let exp_send_from_vec = vec![ExitCode::Ok, ExitCode::ErrPlaceholder, ExitCode::Ok];
    let exp_send_to_vec = vec![ExitCode::Ok, ExitCode::Ok, ExitCode::ErrPlaceholder];
    let exp_collect_vec = vec![
        ExitCode::ErrForbidden,
        ExitCode::ErrPlaceholder,
        ExitCode::ErrPlaceholder,
    ];
    let num_test_cases = dont_settle_vec.len();

    for i in 0..num_test_cases {
        let bs = MemoryDB::default();
        let (mut rt, _sv) = require_create_cannel_with_lanes(&bs, 1);
        rt.epoch = 10;
        let mut state: PState = rt.get_state().unwrap();

        if !dont_settle_vec[i] {
            rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.from);
            rt.expect_validate_caller_addr(&[state.from, state.to]);
            assert!(rt
                .call(
                    &PAYCH_ACTOR_CODE_ID.clone(),
                    Method::Settle as u64,
                    &Serialized::default()
                )
                .is_ok());
            state = rt.get_state().unwrap();
            assert_eq!(state.settling_at, 11);
        }

        // "wait" for SettlingAt epoch
        rt.epoch = 12;

        let sent_to_from = &rt.balance - state.to_send.clone();
        rt.expect_send(
            state.from,
            METHOD_SEND,
            Serialized::default(),
            sent_to_from,
            Serialized::default(),
            exp_send_from_vec[i],
        );
        rt.expect_send(
            state.to,
            METHOD_SEND,
            Serialized::default(),
            state.to_send,
            Serialized::default(),
            exp_send_to_vec[i],
        );

        // Collect.
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.from);
        rt.expect_validate_caller_addr(&[state.from, state.to]);
        let error = rt
            .call(
                &PAYCH_ACTOR_CODE_ID.clone(),
                Method::Collect as u64,
                &Serialized::default(),
            )
            .unwrap_err();
        assert_eq!(exp_collect_vec[i], error.exit_code());
    }
}

fn require_create_cannel_with_lanes<'a, BS: BlockStore>(
    bs: &'a BS,
    num_lanes: u64,
) -> (MockRuntime<'a, BS>, SignedVoucher) {
    let paych_addr = Address::new_id(100);
    let payer_addr = Address::new_id(102);
    let payee_addr = Address::new_id(103);
    let balance = TokenAmount::from(100_000 as u64);
    let recieved = TokenAmount::from(0 as u64);

    let curr_epoch = 2;

    let message = UnsignedMessage::builder()
        .from(*SYSTEM_ACTOR_ADDR)
        .to(paych_addr)
        .gas_limit(1000)
        .build()
        .unwrap();

    let mut rt = MockRuntime::new(bs, message);

    rt.epoch = curr_epoch;
    rt.balance = TokenAmount::from(balance);
    rt.received = TokenAmount::from(recieved);
    rt.set_caller(INIT_ACTOR_CODE_ID.clone(), *INIT_ACTOR_ADDR);
    rt.actor_code_cids
        .insert(payee_addr, ACCOUNT_ACTOR_CODE_ID.clone());
    rt.actor_code_cids
        .insert(payer_addr, ACCOUNT_ACTOR_CODE_ID.clone());
    construct_and_verify(&mut rt, payer_addr, payee_addr);

    let mut last_sv = SignedVoucher::default();
    for i in 0..num_lanes {
        let lane_param = LaneParams {
            epoch_num: curr_epoch,
            from: payer_addr,
            to: payee_addr,
            amt: (i + 1) as i64,
            lane: i as u64,
            nonce: i + 1,
        };

        last_sv = require_add_new_lane(&mut rt, lane_param);
    }

    (rt, last_sv)
}

fn require_add_new_lane<BS: BlockStore>(
    rt: &mut MockRuntime<'_, BS>,
    param: LaneParams,
) -> SignedVoucher {
    let payee_addr = Address::new_id(103);
    let sig = Signature::new_bls(vec![0, 1, 2, 3, 4, 5, 6, 7]);
    // println!("sig is {:?}", sig.bytes());
    let sv = SignedVoucher {
        time_lock_min: param.epoch_num,
        time_lock_max: u64::MAX,
        secret_pre_image: vec![],
        extra: Option::None,
        lane: param.lane,
        nonce: param.nonce,
        amount: BigInt::from(param.amt),
        min_settle_height: 0,
        merges: vec![],
        signature: Some(sig.clone()),
    };
    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), param.from);
    rt.expect_validate_caller_addr(&[param.from, param.to]);
    rt.expect_verify_signature(sig.clone(), payee_addr, to_vec(&sv).unwrap(), ExitCode::Ok);
    println!(" sv vec bytes is {:?}", to_vec(&sv).unwrap());

    let ucp = UpdateChannelStateParams {
        proof: vec![],
        secret: vec![],
        sv: sv,
    };

    assert!(rt
        .call(
            &PAYCH_ACTOR_CODE_ID.clone(),
            Method::UpdateChannelState as u64,
            &Serialized::serialize(ucp).unwrap(),
        )
        .is_ok());

    rt.verify();
    SignedVoucher {
        time_lock_min: param.epoch_num,
        time_lock_max: u64::MAX,
        secret_pre_image: vec![],
        extra: Option::None,
        lane: param.lane,
        nonce: param.nonce,
        amount: BigInt::from(param.amt),
        min_settle_height: 0,
        merges: vec![],
        signature: Some(sig.clone()),
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
    assert!( rt
        .call(
            &PAYCH_ACTOR_CODE_ID,
            METHOD_CONSTRUCTOR,
            &Serialized::serialize(&params).unwrap(),
        )
        .is_ok());
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

fn verify_state<BS: BlockStore>(
    rt: &mut MockRuntime<'_, BS>,
    exp_lanes: i64,
    expected_state: PState,
) {
    let state: PState = rt.get_state().unwrap();
    assert_eq!(expected_state.to, state.to);
    assert_eq!(expected_state.from, state.from);
    assert_eq!(expected_state.min_settle_height, state.min_settle_height);
    assert_eq!(expected_state.settling_at, state.settling_at);
    assert_eq!(expected_state.to_send, state.to_send);

    if exp_lanes > 0 {
        assert_eq!(exp_lanes as u64, state.lane_states.len() as u64);
        assert_eq!(expected_state.lane_states, state.lane_states);
    } else {
        assert_eq!(state.lane_states.len(), 0);
    }
}
