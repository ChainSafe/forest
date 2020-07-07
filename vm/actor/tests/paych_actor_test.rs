// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod common;
use actor::{
    paych::{
        ConstructorParams, LaneState, Merge, Method, ModVerifyParams, PaymentVerifyParams,
        SignedVoucher, State as PState, UpdateChannelStateParams, LANE_LIMIT, SETTLE_DELAY,
    },
    ACCOUNT_ACTOR_CODE_ID, INIT_ACTOR_ADDR, INIT_ACTOR_CODE_ID, MULTISIG_ACTOR_CODE_ID,
    PAYCH_ACTOR_CODE_ID, SYSTEM_ACTOR_ADDR,
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
use num_bigint::BigInt;
use vm::{ExitCode, Serialized, TokenAmount, METHOD_CONSTRUCTOR, METHOD_SEND};

const R_PAYEE_ADDR: u64 = 103;
const R_PAYER_ADDR: u64 = 102;
struct LaneParams {
    epoch_num: ChainEpoch,
    from: Address,
    to: Address,
    amt: i64,
    lane: u64,
    nonce: u64,
}

fn is_ok<'a, BS: BlockStore>(rt: &mut MockRuntime<'a, BS>, method_num: u64, ser: &Serialized) {
    assert!(rt.call(&*PAYCH_ACTOR_CODE_ID, method_num, ser).is_ok());
}

fn expect_error<'a, BS: BlockStore>(
    rt: &mut MockRuntime<'a, BS>,
    method_num: u64,
    ser: &Serialized,
    exp: ExitCode,
) {
    assert_eq!(
        exp,
        rt.call(&*PAYCH_ACTOR_CODE_ID, method_num, ser,)
            .unwrap_err()
            .exit_code()
    );
}

mod paych_constructor {

    use super::*;
    const TEST_PAYCH_ADDR: u64 = 100;
    const TEST_PAYER_ADDR: u64 = 101;
    const TEST_CALLER_ADDR: u64 = 102;

    fn construct_runtime<'a, BS: BlockStore>(bs: &'a BS) -> MockRuntime<'a, BS> {
        let paych_addr = Address::new_id(TEST_PAYCH_ADDR);
        let payer_addr = Address::new_id(TEST_PAYER_ADDR);
        let caller_addr = Address::new_id(TEST_CALLER_ADDR);
        let message = UnsignedMessage::builder()
            .from(*SYSTEM_ACTOR_ADDR)
            .to(paych_addr)
            .build()
            .unwrap();
        let mut rt = MockRuntime::new(bs, message);
        rt.set_caller(INIT_ACTOR_CODE_ID.clone(), caller_addr);
        rt.actor_code_cids
            .insert(payer_addr, ACCOUNT_ACTOR_CODE_ID.clone());
        rt
    }

    #[test]
    fn create_paych_actor_test() {
        let caller_addr = Address::new_id(TEST_CALLER_ADDR);
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        rt.actor_code_cids
            .insert(caller_addr, ACCOUNT_ACTOR_CODE_ID.clone());
        construct_and_verify(&mut rt, Address::new_id(TEST_PAYER_ADDR), caller_addr);
    }

    #[test]
    fn actor_doesnt_exist_test() {
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        rt.expect_validate_caller_type(&[INIT_ACTOR_CODE_ID.clone()]);
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

        let test_cases: Vec<TestCase> = vec![
            TestCase {
                paych_addr: paych_addr,
                caller_code: INIT_ACTOR_CODE_ID.clone(),
                new_actor_code: MULTISIG_ACTOR_CODE_ID.clone(),
                payer_code: ACCOUNT_ACTOR_CODE_ID.clone(),
                expected_exit_code: ExitCode::ErrIllegalArgument,
            },
            TestCase {
                paych_addr: Address::new_secp256k1(&vec![b'A'; 65][..]).unwrap(),
                caller_code: INIT_ACTOR_CODE_ID.clone(),
                new_actor_code: ACCOUNT_ACTOR_CODE_ID.clone(),
                payer_code: ACCOUNT_ACTOR_CODE_ID.clone(),
                expected_exit_code: ExitCode::ErrIllegalArgument,
            },
        ];

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
                .desc("succeds".to_string())
                .sig(sig.clone())
                .exp_exit_code(ExitCode::Ok)
                .build()
                .unwrap(),
            TestCase::builder()
                .desc("fails if new send balance is negative".to_string())
                .amt(-1)
                .sig(sig.clone())
                .exp_exit_code(ExitCode::ErrIllegalState)
                .build()
                .unwrap(),
            TestCase::builder()
                .desc("fails if balance too low".to_string())
                .amt(10)
                .sig(sig.clone())
                .exp_exit_code(ExitCode::ErrIllegalState)
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
                .sig(sig.clone())
                .verify_sig(false)
                .build()
                .unwrap(),
            TestCase::builder()
                .desc("Fails if signing fails".to_string())
                .sig(sig.clone())
                .secret_preimage(vec![0; 2 << 21])
                .build()
                .unwrap(),
        ];

        for test_case in test_cases {
            println!("Test Description {}", test_case.desc);
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
                lane: test_case.lane,
                nonce: test_case.nonce,
                amount: BigInt::from(test_case.amt),
                signature: test_case.sig.clone(),
                ..SignedVoucher::default()
            };

            let ucp = UpdateChannelStateParams::from(sv.clone());
            rt.set_caller(test_case.target_code, payee_addr);
            rt.expect_validate_caller_addr(&[payer_addr, payee_addr]);

            if test_case.sig.is_some() && test_case.secret_preimage.len() == 0 {
                let exp_exit_code = if !test_case.verify_sig {
                    ExitCode::ErrIllegalState
                } else {
                    ExitCode::Ok
                };
                rt.expect_verify_signature(ExpectedVerifySig {
                    sig: sv.clone().signature.unwrap(),
                    signer: payer_addr,
                    plaintext: to_vec(&sv).unwrap(),
                    result: exp_exit_code,
                });
            }

            if test_case.exp_exit_code == ExitCode::Ok {
                is_ok(
                    &mut rt,
                    Method::UpdateChannelState as u64,
                    &Serialized::serialize(ucp).unwrap(),
                );

                let st: PState = rt.get_state().unwrap();
                assert_eq!(st.lane_states.len(), 1);
                let ls = st.lane_states.first().unwrap();
                assert_eq!(sv.amount, ls.redeemed);
                assert_eq!(sv.nonce, ls.nonce);
                assert_eq!(sv.lane, ls.id);
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
        let bs = MemoryDB::default();
        let (mut rt, mut sv) = require_create_cannel_with_lanes(&bs, 1);
        let state: PState = rt.get_state().unwrap();
        let payee_addr = Address::new_id(R_PAYEE_ADDR);

        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), payee_addr);
        rt.expect_validate_caller_addr(&[state.from, state.to]);

        sv.amount = BigInt::from(9);

        let payer_addr = Address::new_id(R_PAYER_ADDR);

        rt.expect_verify_signature(ExpectedVerifySig {
            sig: sv.clone().signature.unwrap(),
            signer: payer_addr,
            plaintext: to_vec(&sv).unwrap(),
            result: ExitCode::Ok,
        });

        is_ok(
            &mut rt,
            Method::UpdateChannelState as u64,
            &Serialized::serialize(UpdateChannelStateParams::from(sv.clone())).unwrap(),
        );

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
        let payee_addr = Address::new_id(R_PAYEE_ADDR);

        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), payee_addr);
        rt.expect_validate_caller_addr(&[state.from, state.to]);

        let initial_amount = state.to_send;
        sv.amount = BigInt::from(9);
        sv.lane = 1;
        let ls_to_update: &LaneState = &state.lane_states[1];
        sv.nonce = ls_to_update.nonce + 1;
        let payer_addr = Address::new_id(R_PAYER_ADDR);

        rt.expect_verify_signature(ExpectedVerifySig {
            sig: sv.clone().signature.unwrap(),
            signer: payer_addr,
            plaintext: to_vec(&sv).unwrap(),
            result: ExitCode::Ok,
        });

        is_ok(
            &mut rt,
            Method::UpdateChannelState as u64,
            &Serialized::serialize(UpdateChannelStateParams::from(sv.clone())).unwrap(),
        );

        rt.verify();

        let state: PState = rt.get_state().unwrap();
        let ls_updated: &LaneState = &state.lane_states[1];
        let big_delta = &sv.amount - &ls_to_update.redeemed;

        let exp_send = big_delta.to_biguint().unwrap() + &initial_amount;
        assert_eq!(exp_send, state.to_send);
        assert_eq!(sv.amount, ls_updated.redeemed);
        assert_eq!(sv.nonce, ls_updated.nonce);
    }
}

mod merge_tests {
    use super::*;

    fn construct_runtime<'a, BS: BlockStore>(
        bs: &'a BS,
        num_lanes: u64,
    ) -> (MockRuntime<'a, BS>, SignedVoucher, PState) {
        let (mut rt, sv) = require_create_cannel_with_lanes(bs, num_lanes);
        let state: PState = rt.get_state().unwrap();
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.from.clone());
        rt.expect_validate_caller_addr(&[state.from, state.to]);
        (rt, sv, state)
    }

    fn failure_end<BS: BlockStore>(
        rt: &mut MockRuntime<BS>,
        sv: SignedVoucher,
        exp_exit_code: ExitCode,
    ) {
        let payee_addr = Address::new_id(R_PAYEE_ADDR);
        rt.expect_verify_signature(ExpectedVerifySig {
            sig: sv.clone().signature.unwrap(),
            signer: payee_addr,
            plaintext: to_vec(&sv).unwrap(),
            result: ExitCode::Ok,
        });
        expect_error(
            rt,
            Method::UpdateChannelState as u64,
            &Serialized::serialize(UpdateChannelStateParams::from(sv.clone())).unwrap(),
            exp_exit_code,
        );
        rt.verify();
    }

    #[test]
    fn merge_success() {
        let num_lanes = 3;
        let bs = MemoryDB::default();
        let (mut rt, mut sv, mut state) = construct_runtime(&bs, num_lanes);

        let merge_to: &LaneState = &state.lane_states[0];
        let merge_from: &LaneState = &state.lane_states[1];
        sv.lane = merge_to.id;
        let merge_nonce = merge_to.nonce + 10;

        sv.merges = vec![Merge {
            lane: merge_from.id,
            nonce: merge_nonce,
        }];
        let payee_addr = Address::new_id(R_PAYEE_ADDR);
        rt.expect_verify_signature(ExpectedVerifySig {
            sig: sv.clone().signature.unwrap(),
            signer: payee_addr,
            plaintext: to_vec(&sv).unwrap(),
            result: ExitCode::Ok,
        });

        is_ok(
            &mut rt,
            Method::UpdateChannelState as u64,
            &Serialized::serialize(UpdateChannelStateParams::from(sv.clone())).unwrap(),
        );
        rt.verify();
        let exp_merge_to = LaneState {
            id: merge_to.id,
            redeemed: sv.amount.clone(),
            nonce: sv.nonce,
        };
        let exp_merge_from = LaneState {
            id: merge_from.id,
            redeemed: merge_from.redeemed.clone(),
            nonce: merge_nonce,
        };
        let redeemed = &merge_from.redeemed + &merge_to.redeemed;
        let exp_delta = &sv.amount - &redeemed;
        state.to_send = exp_delta.to_biguint().unwrap() + &state.to_send;

        state.lane_states = vec![
            exp_merge_to,
            exp_merge_from,
            state.lane_states.pop().unwrap(),
        ];
        verify_state(&mut rt, num_lanes as i64, state);
    }

    #[test]
    fn merge_failue() {
        struct TestCase {
            lane: i32,
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
                exit: ExitCode::ErrIllegalState,
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
            let bs = MemoryDB::default();
            let num_lanes = 2;
            let (mut rt, mut sv, state) = construct_runtime(&bs, num_lanes);

            rt.balance = TokenAmount::from(tc.balance as u64);

            let merge_to: &LaneState = &state.lane_states[0];
            let merge_from: &LaneState = &state.lane_states[tc.lane as usize];
            sv.lane = merge_to.id;
            sv.nonce = tc.voucher;
            sv.merges = vec![Merge {
                lane: merge_from.id,
                nonce: tc.merge,
            }];
            failure_end(&mut rt, sv, tc.exit);
        }
    }

    #[test]
    fn invalid_merge_lane_999() {
        let bs = MemoryDB::default();
        let num_lanes = 2;
        let (mut rt, mut sv, state) = construct_runtime(&bs, num_lanes);

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
        failure_end(&mut rt, sv, ExitCode::ErrIllegalArgument);
    }

    #[test]
    fn lane_limit_exceeded() {
        let bs = MemoryDB::default();
        let num_lanes = LANE_LIMIT as u64;
        let (mut rt, mut sv, _) = construct_runtime(&bs, num_lanes);

        sv.lane += 1;
        sv.nonce += 1;
        sv.amount = BigInt::from(100);
        failure_end(&mut rt, sv, ExitCode::ErrIllegalArgument);
    }
}

mod update_channel_state_extra {
    use super::*;
    const OTHER_ADDR: u64 = 104;

    fn construct_runtime<'a, BS: BlockStore>(
        bs: &'a BS,
        exit_code: ExitCode,
    ) -> (MockRuntime<'a, BS>, SignedVoucher) {
        let (mut rt, mut sv) = require_create_cannel_with_lanes(bs, 1);
        let state: PState = rt.get_state().unwrap();
        let other_addr = Address::new_id(OTHER_ADDR);
        let fake_params = [1, 2, 3, 4];
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.from);
        rt.expect_validate_caller_addr(&[state.from, state.to]);

        sv.extra = Some(ModVerifyParams {
            actor: other_addr,
            method: Method::UpdateChannelState as u64,
            data: Serialized::serialize(fake_params.clone()).unwrap(),
        });
        rt.expect_verify_signature(ExpectedVerifySig {
            sig: sv.clone().signature.unwrap(),
            signer: state.to,
            plaintext: to_vec(&sv).unwrap(),
            result: ExitCode::Ok,
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
    fn extra_call_succeed() {
        let bs = MemoryDB::default();
        let (mut rt, sv) = construct_runtime(&bs, ExitCode::Ok);
        is_ok(
            &mut rt,
            Method::UpdateChannelState as u64,
            &Serialized::serialize(UpdateChannelStateParams::from(sv.clone())).unwrap(),
        );
        rt.verify();
    }

    #[test]
    fn extra_call_fail() {
        let bs = MemoryDB::default();
        let (mut rt, sv) = construct_runtime(&bs, ExitCode::ErrPlaceholder);
        expect_error(
            &mut rt,
            Method::UpdateChannelState as u64,
            &Serialized::serialize(UpdateChannelStateParams::from(sv.clone())).unwrap(),
            ExitCode::ErrPlaceholder,
        );
        rt.verify();
    }
}

mod update_channel_state_settling {
    use super::*;
    #[test]
    fn update_channel_setting() {
        let bs = MemoryDB::default();
        let (mut rt, sv) = require_create_cannel_with_lanes(&bs, 1);
        rt.epoch = 10;
        let state: PState = rt.get_state().unwrap();
        rt.expect_validate_caller_addr(&[state.from, state.to]);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.from);
        is_ok(&mut rt, Method::Settle as u64, &Serialized::default());

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
                exp_settling_at: 12,
            },
        ];

        for tc in test_cases {
            let mut ucp = UpdateChannelStateParams::from(sv.clone());
            ucp.sv.min_settle_height = tc.min_settle;
            rt.expect_validate_caller_addr(&[state.from, state.to]);
            rt.expect_verify_signature(ExpectedVerifySig {
                sig: sv.clone().signature.unwrap(),
                signer: state.to,
                plaintext: to_vec(&ucp.sv).unwrap(),
                result: ExitCode::Ok,
            });
            is_ok(
                &mut rt,
                Method::UpdateChannelState as u64,
                &Serialized::serialize(ucp).unwrap(),
            );
            let new_state: PState = rt.get_state().unwrap();
            assert_eq!(tc.exp_settling_at, new_state.settling_at);
            assert_eq!(tc.exp_min_settle_height, new_state.min_settle_height);
        }
    }
}
mod secret_preimage {
    use super::*;
    #[test]
    fn succeed_correct_secret() {
        let bs = MemoryDB::default();
        let (mut rt, sv) = require_create_cannel_with_lanes(&bs, 1);
        let state: PState = rt.get_state().unwrap();
        rt.expect_validate_caller_addr(&[state.from, state.to]);

        let ucp = UpdateChannelStateParams::from(sv.clone());

        rt.expect_verify_signature(ExpectedVerifySig {
            sig: sv.clone().signature.unwrap(),
            signer: state.to,
            plaintext: to_vec(&sv).unwrap(),
            result: ExitCode::Ok,
        });

        is_ok(
            &mut rt,
            Method::UpdateChannelState as u64,
            &Serialized::serialize(ucp).unwrap(),
        );

        rt.verify();
    }

    #[test]
    fn incorrect_secret() {
        let bs = MemoryDB::default();
        let (mut rt, sv) = require_create_cannel_with_lanes(&bs, 1);

        let state: PState = rt.get_state().unwrap();
        rt.expect_validate_caller_addr(&[state.from, state.to]);

        let mut ucp = UpdateChannelStateParams {
            proof: vec![],
            secret: b"Profesr".to_vec(),
            sv: sv.clone(),
        };
        let mut mag = b"Magneto".to_vec();
        mag.append(&mut vec![0; 25]);
        ucp.sv.secret_pre_image = mag;

        rt.expect_verify_signature(ExpectedVerifySig {
            sig: sv.clone().signature.unwrap(),
            signer: state.to,
            plaintext: to_vec(&sv).unwrap(),
            result: ExitCode::Ok,
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
        let bs = MemoryDB::default();
        let (mut rt, _sv) = require_create_cannel_with_lanes(&bs, 1);
        rt.epoch = EP;
        let mut state: PState = rt.get_state().unwrap();
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.from);
        rt.expect_validate_caller_addr(&[state.from, state.to]);

        is_ok(&mut rt, Method::Settle as u64, &Serialized::default());

        let exp_settling_at = EP + SETTLE_DELAY;
        state = rt.get_state().unwrap();
        assert_eq!(state.settling_at, exp_settling_at);
        assert_eq!(state.min_settle_height, 0);
    }

    #[test]
    fn call_twice() {
        let bs = MemoryDB::default();
        let (mut rt, _sv) = require_create_cannel_with_lanes(&bs, 1);
        rt.epoch = EP;
        let state: PState = rt.get_state().unwrap();
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.from);
        rt.expect_validate_caller_addr(&[state.from, state.to]);
        is_ok(&mut rt, Method::Settle as u64, &Serialized::default());

        rt.expect_validate_caller_addr(&[state.from, state.to]);
        expect_error(
            &mut rt,
            Method::Settle as u64,
            &Serialized::default(),
            ExitCode::ErrIllegalState,
        );
    }

    #[test]
    fn settle_if_height_less() {
        let bs = MemoryDB::default();
        let (mut rt, mut sv) = require_create_cannel_with_lanes(&bs, 1);
        rt.epoch = EP;
        let mut state: PState = rt.get_state().unwrap();

        sv.min_settle_height = (EP + SETTLE_DELAY) + 1;
        let ucp = UpdateChannelStateParams::from(sv.clone());

        rt.expect_validate_caller_addr(&[state.from, state.to]);
        rt.expect_verify_signature(ExpectedVerifySig {
            sig: ucp.sv.clone().signature.unwrap(),
            signer: state.to,
            plaintext: to_vec(&sv).unwrap(),
            result: ExitCode::Ok,
        });
        is_ok(
            &mut rt,
            Method::UpdateChannelState as u64,
            &Serialized::serialize(&ucp).unwrap(),
        );
        state = rt.get_state().unwrap();
        assert_eq!(state.settling_at, 0);
        assert_eq!(state.min_settle_height, ucp.sv.min_settle_height);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.from);
        rt.expect_validate_caller_addr(&[state.from, state.to]);
        is_ok(&mut rt, Method::Settle as u64, &Serialized::default());
        state = rt.get_state().unwrap();
        assert_eq!(state.settling_at, ucp.sv.min_settle_height);
    }
}

mod actor_collect {
    use super::*;

    fn exp_send_multiple<BS: BlockStore>(
        rt: &mut MockRuntime<BS>,
        state: &PState,
        exit_codes: [ExitCode; 2],
    ) {
        rt.epoch = 12;
        let sent_to_from = &rt.balance - state.to_send.clone();
        rt.expect_send(
            state.from,
            METHOD_SEND,
            Serialized::default(),
            sent_to_from,
            Serialized::default(),
            exit_codes[0],
        );
        rt.expect_send(
            state.to,
            METHOD_SEND,
            Serialized::default(),
            state.to_send.to_owned(),
            Serialized::default(),
            exit_codes[1],
        );
        rt.expect_validate_caller_addr(&[state.from, state.to]);
    }

    #[test]
    fn happy_path() {
        let bs = MemoryDB::default();
        let (mut rt, _sv) = require_create_cannel_with_lanes(&bs, 1);
        rt.epoch = 10;
        let mut state: PState = rt.get_state().unwrap();
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.from);
        rt.expect_validate_caller_addr(&[state.from, state.to]);
        is_ok(&mut rt, Method::Settle as u64, &Serialized::default());
        state = rt.get_state().unwrap();

        assert_eq!(state.settling_at, 11);
        rt.expect_validate_caller_addr(&[state.from, state.to]);

        exp_send_multiple(&mut rt, &state, [ExitCode::Ok, ExitCode::Ok]);

        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.to);

        is_ok(&mut rt, Method::Collect as u64, &Serialized::default());
        state = rt.get_state().unwrap();
        assert_eq!(state.to_send, TokenAmount::from(0u8));
    }

    #[test]
    fn actor_collect() {
        struct TestCase {
            dont_settle: bool,
            exp_send_from: ExitCode,
            exp_send_to: ExitCode,
            exp_send_collect: ExitCode,
        }

        let test_cases = vec![
            TestCase {
                dont_settle: true,
                exp_send_from: ExitCode::Ok,
                exp_send_to: ExitCode::Ok,
                exp_send_collect: ExitCode::ErrForbidden,
            },
            TestCase {
                dont_settle: false,
                exp_send_from: ExitCode::ErrPlaceholder,
                exp_send_to: ExitCode::Ok,
                exp_send_collect: ExitCode::ErrPlaceholder,
            },
            TestCase {
                dont_settle: false,
                exp_send_from: ExitCode::Ok,
                exp_send_to: ExitCode::ErrPlaceholder,
                exp_send_collect: ExitCode::ErrPlaceholder,
            },
        ];

        for tc in test_cases {
            let bs = MemoryDB::default();
            let (mut rt, _sv) = require_create_cannel_with_lanes(&bs, 1);
            rt.epoch = 10;
            let mut state: PState = rt.get_state().unwrap();

            if !tc.dont_settle {
                rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.from);
                rt.expect_validate_caller_addr(&[state.from, state.to]);
                is_ok(&mut rt, Method::Settle as u64, &Serialized::default());
                state = rt.get_state().unwrap();
                assert_eq!(state.settling_at, 11);
            }

            exp_send_multiple(&mut rt, &state, [tc.exp_send_from, tc.exp_send_to]);

            rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), state.from);

            expect_error(
                &mut rt,
                Method::Collect as u64,
                &Serialized::default(),
                tc.exp_send_collect,
            );
        }
    }
}

fn require_create_cannel_with_lanes<'a, BS: BlockStore>(
    bs: &'a BS,
    num_lanes: u64,
) -> (MockRuntime<'a, BS>, SignedVoucher) {
    let paych_addr = Address::new_id(100 as u64);
    let payer_addr = Address::new_id(R_PAYER_ADDR);
    let payee_addr = Address::new_id(R_PAYEE_ADDR);
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
    let payee_addr = Address::new_id(103 as u64);
    let sig = Signature::new_bls(vec![0, 1, 2, 3, 4, 5, 6, 7]);
    let sv = SignedVoucher {
        time_lock_min: param.epoch_num,
        time_lock_max: i64::MAX,
        lane: param.lane,
        nonce: param.nonce,
        amount: BigInt::from(param.amt),
        signature: Some(sig.clone()),
        ..SignedVoucher::default()
    };
    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), param.from);
    rt.expect_validate_caller_addr(&[param.from, param.to]);
    rt.expect_verify_signature(ExpectedVerifySig {
        sig: sig.clone(),
        signer: payee_addr,
        plaintext: to_vec(&sv).unwrap(),
        result: ExitCode::Ok,
    });
    is_ok(
        rt,
        Method::UpdateChannelState as u64,
        &Serialized::serialize(UpdateChannelStateParams::from(sv.clone())).unwrap(),
    );
    rt.verify();
    SignedVoucher {
        time_lock_min: param.epoch_num,
        time_lock_max: i64::MAX,
        lane: param.lane,
        nonce: param.nonce,
        amount: BigInt::from(param.amt),
        signature: Some(sig.clone()),
        ..SignedVoucher::default()
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
    is_ok(
        rt,
        METHOD_CONSTRUCTOR,
        &Serialized::serialize(&params).unwrap(),
    );
    rt.verify();
    verify_initial_state(rt, sender, receiver);
}

fn verify_initial_state<BS: BlockStore>(
    rt: &mut MockRuntime<'_, BS>,
    sender: Address,
    receiver: Address,
) {
    let _state: PState = rt.get_state().unwrap();
    let expected_state = PState::new(sender, receiver);
    verify_state(rt, -1, expected_state)
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
