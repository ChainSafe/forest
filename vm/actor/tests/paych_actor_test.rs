// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod common;
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use common::*;
use crypto::Signature;
use derive_builder::Builder;
use forest_actor::{
    paych::{
        ConstructorParams, LaneState, Merge, Method, ModVerifyParams, PaymentVerifyParams,
        SignedVoucher, State as PState, UpdateChannelStateParams, MAX_LANE, SETTLE_DELAY,
    },
    ACCOUNT_ACTOR_CODE_ID, INIT_ACTOR_ADDR, INIT_ACTOR_CODE_ID, MULTISIG_ACTOR_CODE_ID,
    PAYCH_ACTOR_CODE_ID,
};
use ipld_amt::Amt;
use num_bigint::BigInt;
use std::collections::HashMap;
use std::error::Error as StdError;
use vm::{ExitCode, Serialized, TokenAmount, METHOD_CONSTRUCTOR, METHOD_SEND};

const PAYCH_ID: u64 = 100;
const PAYER_ID: u64 = 102;
const PAYEE_ID: u64 = 103;

struct LaneParams {
    epoch_num: ChainEpoch,
    from: Address,
    to: Address,
    amt: BigInt,
    lane: u64,
    nonce: u64,
}

fn call(rt: &mut MockRuntime, method_num: u64, ser: &Serialized) -> Serialized {
    rt.call(&*PAYCH_ACTOR_CODE_ID, method_num, ser).unwrap()
}

fn expect_error(rt: &mut MockRuntime, method_num: u64, ser: &Serialized, exp: ExitCode) {
    let err = rt.call(&*PAYCH_ACTOR_CODE_ID, method_num, ser).unwrap_err();
    assert_eq!(exp, err.exit_code());
}

fn construct_lane_state_amt(rt: &MockRuntime, lss: Vec<LaneState>) -> Cid {
    let mut arr = Amt::new(&rt.store);
    for (i, ls) in (0..).zip(lss.into_iter()) {
        arr.set(i, ls).unwrap();
    }
    arr.flush().unwrap()
}

fn get_lane_state(rt: &MockRuntime, cid: &Cid, lane: usize) -> LaneState {
    let arr: Amt<LaneState, _> = Amt::load(cid, &rt.store).unwrap();

    arr.get(lane).unwrap().unwrap().clone()
}

mod paych_constructor {

    use super::*;
    const TEST_PAYCH_ADDR: u64 = 100;
    const TEST_PAYER_ADDR: u64 = 101;
    const TEST_CALLER_ADDR: u64 = 102;

    fn construct_runtime() -> MockRuntime {
        let paych_addr = Address::new_id(TEST_PAYCH_ADDR);
        let payer_addr = Address::new_id(TEST_PAYER_ADDR);
        let caller_addr = Address::new_id(TEST_CALLER_ADDR);
        let mut actor_code_cids = HashMap::default();
        actor_code_cids.insert(payer_addr, *ACCOUNT_ACTOR_CODE_ID);

        MockRuntime {
            receiver: paych_addr,
            caller: caller_addr,
            caller_type: *INIT_ACTOR_CODE_ID,
            actor_code_cids,
            ..Default::default()
        }
    }

    #[test]
    fn create_paych_actor_test() {
        let caller_addr = Address::new_id(TEST_CALLER_ADDR);
        let mut rt = construct_runtime();
        rt.actor_code_cids
            .insert(caller_addr, *ACCOUNT_ACTOR_CODE_ID);
        construct_and_verify(&mut rt, Address::new_id(TEST_PAYER_ADDR), caller_addr);
    }

    #[test]
    fn actor_doesnt_exist_test() {
        let mut rt = construct_runtime();
        rt.expect_validate_caller_type(vec![*INIT_ACTOR_CODE_ID]);
        let params = ConstructorParams {
            to: Address::new_id(TEST_PAYCH_ADDR),
            from: Address::new_id(TEST_PAYER_ADDR),
        };
        expect_error(
            &mut rt,
            METHOD_CONSTRUCTOR,
            &Serialized::serialize(params).unwrap(),
            ExitCode::ErrIllegalArgument,
        );
    }

    #[test]
    fn actor_constructor_fails() {
        let paych_addr = Address::new_id(TEST_PAYCH_ADDR);
        let payer_addr = Address::new_id(TEST_PAYER_ADDR);
        let caller_addr = Address::new_id(TEST_CALLER_ADDR);

        struct TestCase {
            paych_addr: Address,
            caller_code: Cid,
            new_actor_code: Cid,
            payer_code: Cid,
            expected_exit_code: ExitCode,
        }

        let test_cases: Vec<TestCase> = vec![TestCase {
            paych_addr,
            caller_code: *INIT_ACTOR_CODE_ID,
            new_actor_code: *MULTISIG_ACTOR_CODE_ID,
            payer_code: *ACCOUNT_ACTOR_CODE_ID,
            expected_exit_code: ExitCode::ErrForbidden,
        }];

        for test_case in test_cases {
            let mut actor_code_cids = HashMap::default();
            actor_code_cids.insert(test_case.paych_addr, test_case.new_actor_code);
            actor_code_cids.insert(payer_addr, test_case.payer_code);

            let mut rt = MockRuntime {
                receiver: paych_addr,
                caller: caller_addr,
                caller_type: test_case.caller_code,
                actor_code_cids,
                ..Default::default()
            };

            rt.expect_validate_caller_type(vec![*INIT_ACTOR_CODE_ID]);
            let params = ConstructorParams {
                to: test_case.paych_addr,
                from: Address::new_id(10001),
            };
            expect_error(
                &mut rt,
                METHOD_CONSTRUCTOR,
                &Serialized::serialize(params).unwrap(),
                test_case.expected_exit_code,
            );
        }
    }
}

mod create_lane_tests {
    use super::*;
    const TEST_INIT_ACTOR_ADDR: u64 = 100;
    const PAYCH_ADDR: u64 = 101;
    const PAYER_ADDR: u64 = 102;
    const PAYEE_ADDR: u64 = 103;
    const PAYCH_BALANCE: u64 = 9;

    #[derive(Builder, Debug)]
    #[builder(name = "TestCaseBuilder")]
    struct TestCase {
        #[builder(default = "Address::new_id(PAYCH_ADDR)")]
        payment_channel: Address,
        desc: String,
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
        let init_actor_addr = Address::new_id(TEST_INIT_ACTOR_ADDR);
        let paych_addr = Address::new_id(PAYCH_ADDR);
        let payer_addr = Address::new_id(PAYER_ADDR);
        let payee_addr = Address::new_id(PAYEE_ADDR);
        let paych_balance = TokenAmount::from(PAYCH_BALANCE);
        let sig = Option::Some(Signature::new_bls("doesn't matter".as_bytes().to_vec()));

        let test_cases: Vec<TestCase> = vec![
            TestCase::builder()
                .desc("succeeds".to_string())
                .sig(sig.clone())
                .exp_exit_code(ExitCode::Ok)
                .build()
                .unwrap(),
            TestCase::builder()
                .desc("fails if new send balance is negative".to_string())
                .amt(-1)
                .sig(sig.clone())
                .build()
                .unwrap(),
            TestCase::builder()
                .desc("fails if balance too low".to_string())
                .amt(10)
                .sig(sig.clone())
                .build()
                .unwrap(),
            TestCase::builder()
                .desc("fails is signature is not valid".to_string())
                .sig(Option::None)
                .build()
                .unwrap(),
            TestCase::builder()
                .desc("fails if too early for a voucher".to_string())
                .tl_min(10)
                .sig(sig.clone())
                .build()
                .unwrap(),
            TestCase::builder()
                .desc("fails is beyond timelockmax".to_string())
                .epoch(10)
                .tl_max(5)
                .sig(sig.clone())
                .build()
                .unwrap(),
            TestCase::builder()
                .desc("fails if signature is not verified".to_string())
                .sig(sig)
                .verify_sig(false)
                .build()
                .unwrap(),
            // TODO this should fail with byte array max from cbor gen (pre image serialization)
            // TestCase::builder()
            //     .desc("Fails if signing fails".to_string())
            //     .sig(sig.clone())
            //     .secret_preimage(vec![0; 2 << 21])
            //     .build()
            //     .unwrap(),
        ];

        for test_case in test_cases {
            println!("Test Description {}", test_case.desc);

            let mut actor_code_cids = HashMap::default();
            actor_code_cids.insert(payee_addr, *ACCOUNT_ACTOR_CODE_ID);
            actor_code_cids.insert(payer_addr, *ACCOUNT_ACTOR_CODE_ID);

            let mut rt = MockRuntime {
                receiver: paych_addr,
                caller: init_actor_addr,
                caller_type: *INIT_ACTOR_CODE_ID,
                actor_code_cids,
                epoch: test_case.epoch,
                balance: paych_balance.clone(),
                ..Default::default()
            };

            construct_and_verify(&mut rt, payer_addr, payee_addr);

            let sv = SignedVoucher {
                time_lock_min: test_case.tl_min,
                time_lock_max: test_case.tl_max,
                secret_pre_image: test_case.secret_preimage.clone(),
                lane: test_case.lane as usize,
                nonce: test_case.nonce,
                amount: BigInt::from(test_case.amt),
                signature: test_case.sig.clone(),
                channel_addr: test_case.payment_channel,
                extra: Default::default(),
                min_settle_height: Default::default(),
                merges: Default::default(),
            };

            let ucp = UpdateChannelStateParams::from(sv.clone());
            rt.set_caller(test_case.target_code, payee_addr);
            rt.expect_validate_caller_addr(vec![payer_addr, payee_addr]);

            if test_case.sig.is_some() && test_case.secret_preimage.is_empty() {
                let exp_exit_code = if !test_case.verify_sig {
                    Err(Box::<dyn StdError>::from("bad signature".to_string()))
                } else {
                    Ok(())
                };
                rt.expect_verify_signature(ExpectedVerifySig {
                    sig: sv.clone().signature.unwrap(),
                    signer: payer_addr,
                    plaintext: sv.signing_bytes().unwrap(),
                    result: exp_exit_code,
                });
            }

            if test_case.exp_exit_code == ExitCode::Ok {
                call(
                    &mut rt,
                    Method::UpdateChannelState as u64,
                    &Serialized::serialize(ucp).unwrap(),
                );

                let st: PState = rt.get_state().unwrap();
                let l_states = Amt::<LaneState, _>::load(&st.lane_states, &rt.store).unwrap();
                assert_eq!(l_states.count(), 1);

                let ls = l_states.get(sv.lane).unwrap().unwrap();
                assert_eq!(sv.amount, ls.redeemed);
                assert_eq!(sv.nonce, ls.nonce);
            } else {
                expect_error(
                    &mut rt,
                    Method::UpdateChannelState as u64,
                    &Serialized::serialize(ucp).unwrap(),
                    test_case.exp_exit_code,
                );
                verify_initial_state(&mut rt, payer_addr, payee_addr);
            }
            rt.verify();
        }
    }
}

mod update_channel_state_redeem {
    use super::*;

    #[test]
    fn redeem_voucher_one_lane() {
        let (mut rt, mut sv) = require_create_cannel_with_lanes(1);
        let state: PState = rt.get_state().unwrap();
        let payee_addr = Address::new_id(PAYEE_ID);

        rt.set_caller(*ACCOUNT_ACTOR_CODE_ID, payee_addr);
        rt.expect_validate_caller_addr(vec![state.from, state.to]);

        sv.amount = BigInt::from(9);

        let payer_addr = Address::new_id(PAYER_ID);

        rt.expect_verify_signature(ExpectedVerifySig {
            sig: sv.clone().signature.unwrap(),
            signer: payer_addr,
            plaintext: sv.signing_bytes().unwrap(),
            result: Ok(()),
        });

        call(
            &mut rt,
            Method::UpdateChannelState as u64,
            &Serialized::serialize(UpdateChannelStateParams::from(sv)).unwrap(),
        );

        rt.verify();
        let exp_ls = LaneState {
            redeemed: BigInt::from(9),
            nonce: 2,
        };
        let exp_state = PState {
            from: state.from,
            to: state.to,
            to_send: TokenAmount::from(9),
            settling_at: state.settling_at,
            min_settle_height: state.min_settle_height,
            lane_states: construct_lane_state_amt(&rt, vec![exp_ls]),
        };
        verify_state(&mut rt, Some(1), exp_state);
    }

    #[test]
    fn redeem_voucher_correct_lane() {
        let (mut rt, mut sv) = require_create_cannel_with_lanes(3);
        let state: PState = rt.get_state().unwrap();
        let payee_addr = Address::new_id(PAYEE_ID);

        rt.set_caller(*ACCOUNT_ACTOR_CODE_ID, payee_addr);
        rt.expect_validate_caller_addr(vec![state.from, state.to]);

        let initial_amount = state.to_send;
        sv.amount = BigInt::from(9);
        sv.lane = 1;

        let ls_to_update: LaneState = get_lane_state(&rt, &state.lane_states, sv.lane);
        sv.nonce = ls_to_update.nonce + 1;
        let payer_addr = Address::new_id(PAYER_ID);

        rt.expect_verify_signature(ExpectedVerifySig {
            sig: sv.clone().signature.unwrap(),
            signer: payer_addr,
            plaintext: sv.signing_bytes().unwrap(),
            result: Ok(()),
        });

        call(
            &mut rt,
            Method::UpdateChannelState as u64,
            &Serialized::serialize(UpdateChannelStateParams::from(sv.clone())).unwrap(),
        );

        rt.verify();

        let state: PState = rt.get_state().unwrap();
        let ls_updated: LaneState = get_lane_state(&rt, &state.lane_states, sv.lane);
        let big_delta = &sv.amount - &ls_to_update.redeemed;

        let exp_send = big_delta + &initial_amount;
        assert_eq!(exp_send, state.to_send);
        assert_eq!(sv.amount, ls_updated.redeemed);
        assert_eq!(sv.nonce, ls_updated.nonce);
    }
}

mod merge_tests {
    use super::*;

    fn construct_runtime(num_lanes: u64) -> (MockRuntime, SignedVoucher, PState) {
        let (mut rt, sv) = require_create_cannel_with_lanes(num_lanes);
        let state: PState = rt.get_state().unwrap();
        rt.set_caller(*ACCOUNT_ACTOR_CODE_ID, state.from);
        rt.expect_validate_caller_addr(vec![state.from, state.to]);
        (rt, sv, state)
    }

    fn failure_end(rt: &mut MockRuntime, sv: SignedVoucher, exp_exit_code: ExitCode) {
        let payee_addr = Address::new_id(PAYEE_ID);
        rt.expect_verify_signature(ExpectedVerifySig {
            sig: sv.clone().signature.unwrap(),
            signer: payee_addr,
            plaintext: sv.signing_bytes().unwrap(),
            result: Ok(()),
        });
        expect_error(
            rt,
            Method::UpdateChannelState as u64,
            &Serialized::serialize(UpdateChannelStateParams::from(sv)).unwrap(),
            exp_exit_code,
        );
        rt.verify();
    }

    #[test]
    fn merge_success() {
        let num_lanes = 3;
        let (mut rt, mut sv, mut state) = construct_runtime(num_lanes);

        let merge_to: LaneState = get_lane_state(&rt, &state.lane_states, 0);
        let merge_from: LaneState = get_lane_state(&rt, &state.lane_states, 1);

        sv.lane = 0;
        let merge_nonce = merge_to.nonce + 10;

        sv.merges = vec![Merge {
            lane: 1,
            nonce: merge_nonce,
        }];
        let payee_addr = Address::new_id(PAYEE_ID);
        rt.expect_verify_signature(ExpectedVerifySig {
            sig: sv.clone().signature.unwrap(),
            signer: payee_addr,
            plaintext: sv.signing_bytes().unwrap(),
            result: Ok(()),
        });

        call(
            &mut rt,
            Method::UpdateChannelState as u64,
            &Serialized::serialize(UpdateChannelStateParams::from(sv.clone())).unwrap(),
        );
        rt.verify();
        let exp_merge_to = LaneState {
            redeemed: sv.amount.clone(),
            nonce: sv.nonce,
        };
        let exp_merge_from = LaneState {
            redeemed: merge_from.redeemed.clone(),
            nonce: merge_nonce,
        };
        let redeemed = &merge_from.redeemed + &merge_to.redeemed;
        let exp_delta = &sv.amount - &redeemed;
        state.to_send = exp_delta + &state.to_send;

        state.lane_states = construct_lane_state_amt(
            &rt,
            vec![
                exp_merge_to,
                exp_merge_from,
                get_lane_state(&rt, &state.lane_states, 2),
            ],
        );

        verify_state(&mut rt, Some(num_lanes), state);
    }

    #[test]
    fn merge_failure() {
        struct TestCase {
            lane: u64,
            voucher: u64,
            balance: i32,
            merge: u64,
            exit: ExitCode,
        }
        let test_cases = vec![
            TestCase {
                lane: 1,
                voucher: 10,
                balance: 0,
                merge: 1,
                exit: ExitCode::ErrIllegalArgument,
            },
            TestCase {
                lane: 1,
                voucher: 0,
                balance: 0,
                merge: 10,
                exit: ExitCode::ErrIllegalArgument,
            },
            TestCase {
                lane: 1,
                voucher: 10,
                balance: 1,
                merge: 10,
                exit: ExitCode::ErrIllegalArgument,
            },
            TestCase {
                lane: 0,
                voucher: 10,
                balance: 0,
                merge: 10,
                exit: ExitCode::ErrIllegalArgument,
            },
        ];

        for tc in test_cases {
            let num_lanes = 2;
            let (mut rt, mut sv, state) = construct_runtime(num_lanes);

            rt.balance = TokenAmount::from(tc.balance as u64);

            sv.lane = 0;
            sv.nonce = tc.voucher;
            sv.merges = vec![Merge {
                lane: tc.lane as usize,
                nonce: tc.merge,
            }];
            rt.set_caller(*ACCOUNT_ACTOR_CODE_ID, state.from);
            failure_end(&mut rt, sv, tc.exit);
        }
    }

    #[test]
    fn invalid_merge_lane_999() {
        let num_lanes = 2;
        let (mut rt, mut sv) = require_create_cannel_with_lanes(num_lanes);
        let state: PState = rt.get_state().unwrap();

        sv.lane = 0;
        sv.nonce = 10;
        sv.merges = vec![Merge {
            lane: 999,
            nonce: sv.nonce,
        }];
        rt.set_caller(*ACCOUNT_ACTOR_CODE_ID, state.from);
        rt.expect_validate_caller_addr(vec![state.from, state.to]);
        rt.expect_verify_signature(ExpectedVerifySig {
            plaintext: sv.signing_bytes().unwrap(),
            sig: sv.signature.clone().unwrap(),
            signer: Address::new_id(PAYEE_ID),
            result: Ok(()),
        });
        failure_end(&mut rt, sv, ExitCode::ErrIllegalArgument);
    }

    #[test]
    fn lane_limit_exceeded() {
        let (mut rt, mut sv, _) = construct_runtime(1);

        sv.lane = MAX_LANE as usize + 1;
        sv.nonce += 1;
        sv.amount = BigInt::from(100);
        failure_end(&mut rt, sv, ExitCode::ErrIllegalArgument);
    }
}

mod update_channel_state_extra {
    use super::*;
    const OTHER_ADDR: u64 = 104;

    fn construct_runtime(exit_code: ExitCode) -> (MockRuntime, SignedVoucher) {
        let (mut rt, mut sv) = require_create_cannel_with_lanes(1);
        let state: PState = rt.get_state().unwrap();
        let other_addr = Address::new_id(OTHER_ADDR);
        let fake_params = [1, 2, 3, 4];
        rt.set_caller(*ACCOUNT_ACTOR_CODE_ID, state.from);
        rt.expect_validate_caller_addr(vec![state.from, state.to]);

        sv.extra = Some(ModVerifyParams {
            actor: other_addr,
            method: Method::UpdateChannelState as u64,
            data: Serialized::serialize(fake_params).unwrap(),
        });
        rt.expect_verify_signature(ExpectedVerifySig {
            sig: sv.clone().signature.unwrap(),
            signer: state.to,
            plaintext: sv.signing_bytes().unwrap(),
            result: Ok(()),
        });
        let exp_send_params = PaymentVerifyParams {
            extra: Serialized::serialize(fake_params.to_vec()).unwrap(),
            proof: vec![],
        };

        rt.expect_send(
            other_addr,
            Method::UpdateChannelState as u64,
            Serialized::serialize(exp_send_params).unwrap(),
            TokenAmount::from(0u8),
            Serialized::default(),
            exit_code,
        );
        (rt, sv)
    }
    #[test]
    #[ignore = "old functionality -- test framework needs to be updated"]
    fn extra_call_succeed() {
        let (mut rt, sv) = construct_runtime(ExitCode::Ok);
        call(
            &mut rt,
            Method::UpdateChannelState as u64,
            &Serialized::serialize(UpdateChannelStateParams::from(sv)).unwrap(),
        );
        rt.verify();
    }

    #[test]
    #[ignore = "old functionality -- test framework needs to be updated"]
    fn extra_call_fail() {
        let (mut rt, sv) = construct_runtime(ExitCode::ErrPlaceholder);
        expect_error(
            &mut rt,
            Method::UpdateChannelState as u64,
            &Serialized::serialize(UpdateChannelStateParams::from(sv)).unwrap(),
            ExitCode::ErrPlaceholder,
        );
        rt.verify();
    }
}

#[test]
fn update_channel_settling() {
    let (mut rt, sv) = require_create_cannel_with_lanes(1);
    rt.epoch = 10;
    let state: PState = rt.get_state().unwrap();
    rt.expect_validate_caller_addr(vec![state.from, state.to]);
    rt.set_caller(*ACCOUNT_ACTOR_CODE_ID, state.from);
    call(&mut rt, Method::Settle as u64, &Serialized::default());

    let exp_settling_at = SETTLE_DELAY + 10;
    let state: PState = rt.get_state().unwrap();
    assert_eq!(exp_settling_at, state.settling_at);
    assert_eq!(state.min_settle_height, 0);

    struct TestCase {
        min_settle: i64,
        exp_min_settle_height: i64,
        exp_settling_at: i64,
    }
    let test_cases = vec![
        TestCase {
            min_settle: 0,
            exp_min_settle_height: state.min_settle_height,
            exp_settling_at: state.settling_at,
        },
        TestCase {
            min_settle: 2,
            exp_min_settle_height: 2,
            exp_settling_at: state.settling_at,
        },
        TestCase {
            min_settle: 12,
            exp_min_settle_height: 12,
            exp_settling_at: state.settling_at,
        },
        TestCase {
            min_settle: state.settling_at + 1,
            exp_min_settle_height: state.settling_at + 1,
            exp_settling_at: state.settling_at + 1,
        },
    ];

    let mut ucp = UpdateChannelStateParams::from(sv.clone());
    for tc in test_cases {
        ucp.sv.min_settle_height = tc.min_settle;
        rt.expect_validate_caller_addr(vec![state.from, state.to]);
        rt.expect_verify_signature(ExpectedVerifySig {
            sig: sv.clone().signature.unwrap(),
            signer: state.to,
            plaintext: ucp.sv.signing_bytes().unwrap(),
            result: Ok(()),
        });
        call(
            &mut rt,
            Method::UpdateChannelState as u64,
            &Serialized::serialize(&ucp).unwrap(),
        );
        let new_state: PState = rt.get_state().unwrap();
        assert_eq!(tc.exp_settling_at, new_state.settling_at);
        assert_eq!(tc.exp_min_settle_height, new_state.min_settle_height);
        ucp.sv.nonce += 1;
    }
}

mod secret_preimage {
    use super::*;
    #[test]
    fn succeed_correct_secret() {
        let (mut rt, sv) = require_create_cannel_with_lanes(1);
        let state: PState = rt.get_state().unwrap();
        rt.expect_validate_caller_addr(vec![state.from, state.to]);

        let ucp = UpdateChannelStateParams::from(sv.clone());

        rt.expect_verify_signature(ExpectedVerifySig {
            sig: sv.clone().signature.unwrap(),
            signer: state.to,
            plaintext: sv.signing_bytes().unwrap(),
            result: Ok(()),
        });

        call(
            &mut rt,
            Method::UpdateChannelState as u64,
            &Serialized::serialize(ucp).unwrap(),
        );

        rt.verify();
    }

    #[test]
    fn incorrect_secret() {
        let (mut rt, sv) = require_create_cannel_with_lanes(1);

        let state: PState = rt.get_state().unwrap();

        let mut ucp = UpdateChannelStateParams {
            secret: b"Profesr".to_vec(),
            sv: sv.clone(),
        };
        let mut mag = b"Magneto".to_vec();
        mag.append(&mut vec![0; 25]);
        ucp.sv.secret_pre_image = mag;

        rt.expect_validate_caller_addr(vec![state.from, state.to]);
        rt.expect_verify_signature(ExpectedVerifySig {
            sig: sv.signature.unwrap(),
            signer: state.to,
            plaintext: ucp.sv.signing_bytes().unwrap(),
            result: Ok(()),
        });
        expect_error(
            &mut rt,
            Method::UpdateChannelState as u64,
            &Serialized::serialize(ucp).unwrap(),
            ExitCode::ErrIllegalArgument,
        );

        rt.verify();
    }
}

mod actor_settle {
    use super::*;
    const EP: i64 = 10;
    #[test]
    fn adjust_settling_at() {
        let (mut rt, _sv) = require_create_cannel_with_lanes(1);
        rt.epoch = EP;
        let mut state: PState = rt.get_state().unwrap();
        rt.set_caller(*ACCOUNT_ACTOR_CODE_ID, state.from);
        rt.expect_validate_caller_addr(vec![state.from, state.to]);

        call(&mut rt, Method::Settle as u64, &Serialized::default());

        let exp_settling_at = EP + SETTLE_DELAY;
        state = rt.get_state().unwrap();
        assert_eq!(state.settling_at, exp_settling_at);
        assert_eq!(state.min_settle_height, 0);
    }

    #[test]
    fn call_twice() {
        let (mut rt, _sv) = require_create_cannel_with_lanes(1);
        rt.epoch = EP;
        let state: PState = rt.get_state().unwrap();
        rt.set_caller(*ACCOUNT_ACTOR_CODE_ID, state.from);
        rt.expect_validate_caller_addr(vec![state.from, state.to]);
        call(&mut rt, Method::Settle as u64, &Serialized::default());

        rt.expect_validate_caller_addr(vec![state.from, state.to]);
        expect_error(
            &mut rt,
            Method::Settle as u64,
            &Serialized::default(),
            ExitCode::ErrIllegalState,
        );
    }

    #[test]
    fn settle_if_height_less() {
        let (mut rt, mut sv) = require_create_cannel_with_lanes(1);
        rt.epoch = EP;
        let mut state: PState = rt.get_state().unwrap();

        sv.min_settle_height = (EP + SETTLE_DELAY) + 1;
        let ucp = UpdateChannelStateParams::from(sv.clone());

        rt.expect_validate_caller_addr(vec![state.from, state.to]);
        rt.expect_verify_signature(ExpectedVerifySig {
            sig: ucp.sv.signature.clone().unwrap(),
            signer: state.to,
            plaintext: sv.signing_bytes().unwrap(),
            result: Ok(()),
        });
        call(
            &mut rt,
            Method::UpdateChannelState as u64,
            &Serialized::serialize(&ucp).unwrap(),
        );

        state = rt.get_state().unwrap();
        assert_eq!(state.settling_at, 0);
        assert_eq!(state.min_settle_height, ucp.sv.min_settle_height);

        // Settle.
        rt.set_caller(*ACCOUNT_ACTOR_CODE_ID, state.from);
        rt.expect_validate_caller_addr(vec![state.from, state.to]);
        call(&mut rt, Method::Settle as u64, &Serialized::default());

        state = rt.get_state().unwrap();
        assert_eq!(state.settling_at, ucp.sv.min_settle_height);
    }
}

mod actor_collect {
    use super::*;

    #[test]
    fn happy_path() {
        let (mut rt, _sv) = require_create_cannel_with_lanes(1);
        let curr_epoch: ChainEpoch = 10;
        rt.epoch = curr_epoch;
        let st: PState = rt.get_state().unwrap();

        // Settle.
        rt.set_caller(*ACCOUNT_ACTOR_CODE_ID, st.from);
        rt.expect_validate_caller_addr(vec![st.from, st.to]);
        call(&mut rt, Method::Settle as u64, &Default::default());

        let st: PState = rt.get_state().unwrap();
        assert_eq!(st.settling_at, SETTLE_DELAY + curr_epoch);
        rt.expect_validate_caller_addr(vec![st.from, st.to]);

        // wait for settlingat epoch
        rt.epoch = st.settling_at + 1;

        rt.expect_send(
            st.to,
            METHOD_SEND,
            Default::default(),
            st.to_send.clone(),
            Default::default(),
            ExitCode::Ok,
        );

        // Collect.
        rt.set_caller(*ACCOUNT_ACTOR_CODE_ID, st.to);
        rt.expect_validate_caller_addr(vec![st.from, st.to]);
        rt.expect_delete_actor(st.from);
        let res = call(&mut rt, Method::Collect as u64, &Default::default());
        assert_eq!(res, Serialized::default());
    }

    #[test]
    fn actor_collect() {
        struct TestCase {
            dont_settle: bool,
            exp_send_to: ExitCode,
            exp_collect_exit: ExitCode,
        }

        let test_cases = vec![
            // fails if not settling with: payment channel not settling or settled
            TestCase {
                dont_settle: true,
                exp_send_to: ExitCode::Ok,
                exp_collect_exit: ExitCode::ErrForbidden,
            },
            // fails if Failed to send funds to `To`
            TestCase {
                dont_settle: false,
                exp_send_to: ExitCode::ErrPlaceholder,
                exp_collect_exit: ExitCode::ErrPlaceholder,
            },
        ];

        for tc in test_cases {
            let (mut rt, _sv) = require_create_cannel_with_lanes(1);
            rt.epoch = 10;
            let mut state: PState = rt.get_state().unwrap();

            if !tc.dont_settle {
                rt.set_caller(*ACCOUNT_ACTOR_CODE_ID, state.from);
                rt.expect_validate_caller_addr(vec![state.from, state.to]);
                call(&mut rt, Method::Settle as u64, &Serialized::default());
                state = rt.get_state().unwrap();
                assert_eq!(state.settling_at, SETTLE_DELAY + rt.epoch);
            }

            // "wait" for SettlingAt epoch
            rt.epoch = state.settling_at + 1;
            rt.expect_send(
                state.to,
                METHOD_SEND,
                Default::default(),
                state.to_send.clone(),
                Default::default(),
                tc.exp_send_to,
            );

            // Collect.
            rt.set_caller(*ACCOUNT_ACTOR_CODE_ID, state.from);
            rt.expect_validate_caller_addr(vec![state.from, state.to]);
            expect_error(
                &mut rt,
                Method::Collect as u64,
                &Serialized::default(),
                tc.exp_collect_exit,
            );
        }
    }
}

fn require_create_cannel_with_lanes(num_lanes: u64) -> (MockRuntime, SignedVoucher) {
    let paych_addr = Address::new_id(100);
    let payer_addr = Address::new_id(PAYER_ID);
    let payee_addr = Address::new_id(PAYEE_ID);
    let balance = TokenAmount::from(100_000);
    let received = TokenAmount::from(0);
    let curr_epoch = 2;

    let mut actor_code_cids = HashMap::default();
    actor_code_cids.insert(payee_addr, *ACCOUNT_ACTOR_CODE_ID);
    actor_code_cids.insert(payer_addr, *ACCOUNT_ACTOR_CODE_ID);

    let mut rt = MockRuntime {
        receiver: paych_addr,
        caller: *INIT_ACTOR_ADDR,
        caller_type: *INIT_ACTOR_CODE_ID,
        actor_code_cids,
        received,
        balance,
        epoch: curr_epoch,
        ..Default::default()
    };

    construct_and_verify(&mut rt, payer_addr, payee_addr);

    let mut last_sv = None;
    for i in 0..num_lanes {
        let lane_param = LaneParams {
            epoch_num: curr_epoch,
            from: payer_addr,
            to: payee_addr,
            amt: (BigInt::from(i) + 1),
            lane: i as u64,
            nonce: i + 1,
        };

        last_sv = Some(require_add_new_lane(&mut rt, lane_param));
    }

    (rt, last_sv.unwrap())
}

fn require_add_new_lane(rt: &mut MockRuntime, param: LaneParams) -> SignedVoucher {
    let payee_addr = Address::new_id(103_u64);
    let sig = Signature::new_bls(vec![0, 1, 2, 3, 4, 5, 6, 7]);
    let mut sv = SignedVoucher {
        time_lock_min: param.epoch_num,
        time_lock_max: i64::MAX,
        lane: param.lane as usize,
        nonce: param.nonce,
        amount: param.amt.clone(),
        signature: Some(sig.clone()),
        secret_pre_image: Default::default(),
        channel_addr: Address::new_id(PAYCH_ID),
        extra: Default::default(),
        min_settle_height: Default::default(),
        merges: Default::default(),
    };
    rt.set_caller(*ACCOUNT_ACTOR_CODE_ID, param.from);
    rt.expect_validate_caller_addr(vec![param.from, param.to]);
    rt.expect_verify_signature(ExpectedVerifySig {
        sig,
        signer: payee_addr,
        plaintext: sv.signing_bytes().unwrap(),
        result: Ok(()),
    });
    call(
        rt,
        Method::UpdateChannelState as u64,
        &Serialized::serialize(UpdateChannelStateParams::from(sv.clone())).unwrap(),
    );
    rt.verify();
    sv.nonce += 1;
    sv
}

fn construct_and_verify(rt: &mut MockRuntime, sender: Address, receiver: Address) {
    let params = ConstructorParams {
        from: sender,
        to: receiver,
    };
    rt.expect_validate_caller_type(vec![*INIT_ACTOR_CODE_ID]);
    call(
        rt,
        METHOD_CONSTRUCTOR,
        &Serialized::serialize(&params).unwrap(),
    );
    rt.verify();
    verify_initial_state(rt, sender, receiver);
}

fn verify_initial_state(rt: &mut MockRuntime, sender: Address, receiver: Address) {
    let _state: PState = rt.get_state().unwrap();
    let empt_arr_cid = Amt::<(), _>::new(&rt.store).flush().unwrap();
    let expected_state = PState::new(sender, receiver, empt_arr_cid);
    verify_state(rt, None, expected_state)
}

fn verify_state(rt: &mut MockRuntime, exp_lanes: Option<u64>, expected_state: PState) {
    let state: PState = rt.get_state().unwrap();
    assert_eq!(expected_state.to, state.to);
    assert_eq!(expected_state.from, state.from);
    assert_eq!(expected_state.min_settle_height, state.min_settle_height);
    assert_eq!(expected_state.settling_at, state.settling_at);
    assert_eq!(expected_state.to_send, state.to_send);
    if let Some(exp_lanes) = exp_lanes {
        assert_lane_states_length(rt, &state.lane_states, exp_lanes);
        assert_eq!(expected_state.lane_states, state.lane_states);
    } else {
        assert_lane_states_length(rt, &state.lane_states, 0);
    }
}

fn assert_lane_states_length(rt: &MockRuntime, cid: &Cid, l: u64) {
    let arr = Amt::<LaneState, _>::load(cid, &rt.store).unwrap();
    assert_eq!(arr.count(), l as usize);
}
