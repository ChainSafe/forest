// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[macro_use]
extern crate lazy_static;
mod common;

use actor::{
    init, reward, make_map_with_root, power, Multimap, ACCOUNT_ACTOR_CODE_ID, CALLER_TYPES_SIGNABLE,
    INIT_ACTOR_ADDR, MINER_ACTOR_CODE_ID, MULTISIG_ACTOR_CODE_ID, POWER_ACTOR_CODE_ID,
    STORAGE_POWER_ACTOR_ADDR, SYSTEM_ACTOR_ADDR, SYSTEM_ACTOR_CODE_ID, CRON_ACTOR_ADDR, REWARD_ACTOR_ADDR, CRON_ACTOR_CODE_ID
};
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use common::*;
use encoding::{de::DeserializeOwned, ser::Serialize, BytesDe};
use fil_types::{RegisteredSealProof, StoragePower};
use ipld_blockstore::BlockStore;
use ipld_hamt::BytesKey;
use ipld_hamt::Hamt;
use libp2p::Multiaddr;
use num_bigint::bigint_ser::BigIntSer;
use vm::{ActorError, ExitCode, Serialized, TokenAmount, METHOD_CONSTRUCTOR};

lazy_static! {
    static ref OWNER: Address = Address::new_id(101);
    static ref ACTR: Address = Address::new_actor("actor".as_bytes());
    static ref POWER_UNIT: TokenAmount = RegisteredSealProof::StackedDRG32GiBV1
        .min_miner_consensus_power()
        .unwrap();
    static ref SMALL_POWER_UNIT: TokenAmount = TokenAmount::from(1_000_000);
    static ref MINER_1: Address = Address::new_id(111);
    static ref MINER_2: Address = Address::new_id(112);
}

const MINER_1_ID: u64 = 1;

mod test_construction {

    use super::*;

    #[test]
    fn simple_construction() {
        let _ = construct_and_verify();
    }

    #[test]
    fn create_miner_test() {
        let mut rt = construct_and_verify();

        let _ = create_miner(
            &mut rt,
            OWNER.clone(),
            OWNER.clone(),
            MINER_1.clone(),
            ACTR.clone(),
            vec![BytesDe(vec![1])],
            RegisteredSealProof::StackedDRG2KiBV1,
            TokenAmount::from(10),
            BytesDe("miner".as_bytes().to_owned()),
        );
        let state: power::State = rt.get_state().unwrap();
        assert_eq!(1, state.miner_count);
        assert_eq!(StoragePower::default(), state.total_quality_adj_power);
        assert_eq!(StoragePower::default(), state.total_raw_byte_power);
        assert_eq!(0, state.miner_above_min_power_count);

        verify_claim_size(&state.claims, &rt.store, 1);
        verify_cron_size(&state.cron_event_queue, &rt.store, 0);

        let claim_keys = collect_claim_keys(&state.claims, &rt.store).unwrap();
        let claims: Hamt<_, power::Claim> = make_map_with_root(&state.claims, &rt.store).unwrap();
        assert_eq!(
            power::Claim::default(),
            claims.get(&claim_keys[0]).unwrap().unwrap()
        );
    }
}

mod test_create_miner_failures {

    use super::*;

    #[test]
    fn fails_when_caller_is_not_of_signable_type() {
        let mut rt = construct_and_verify();
        rt.set_caller(MINER_ACTOR_CODE_ID.clone(), OWNER.clone());
        rt.expect_validate_caller_type(CALLER_TYPES_SIGNABLE.to_vec());
        let params = power::CreateMinerParams {
            owner: OWNER.clone(),
            worker: OWNER.clone(),
            control_addresses: vec![],
            peer: BytesDe("miner".as_bytes().to_owned()),
            seal_proof_type: RegisteredSealProof::StackedDRG2KiBV1,
            multiaddrs: vec![BytesDe(vec![1])],
        };
        check_call_fail(
            &mut rt,
            power::Method::CreateMiner as u64,
            &Serialized::serialize(params).unwrap(),
            ExitCode::SysErrForbidden,
        );
        rt.verify();
    }

    #[test]
    fn fails_if_send_to_init_actor_fails() {
        let mut rt = construct_and_verify();
        let value = TokenAmount::from(10);
        let miner_params = power::CreateMinerParams {
            owner: OWNER.clone(),
            worker: OWNER.clone(),
            control_addresses: vec![],
            seal_proof_type: RegisteredSealProof::StackedDRG2KiBV1,
            peer: BytesDe("miner".as_bytes().to_owned()),
            multiaddrs: vec![BytesDe(vec![1])],
        };
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), OWNER.clone());
        rt.set_balance(value.clone());
        rt.value_received = value.clone();

        rt.expect_validate_caller_type(vec![
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);

        let constructor_params = Serialized::serialize(miner_params).unwrap();
        let msg_params = Serialized::serialize(init::ExecParams {
            code_cid: MINER_ACTOR_CODE_ID.clone(),
            constructor_params: constructor_params.clone(),
        })
        .unwrap();
        let exp_return = Serialized::serialize(init::ExecReturn {
            id_address: Address::new_id(1475),
            robust_address: Address::new_actor("test".as_bytes()),
        })
        .unwrap();
        rt.expect_send(
            INIT_ACTOR_ADDR.clone(),
            init::Method::Exec as u64,
            msg_params,
            value,
            exp_return,
            ExitCode::ErrInsufficientFunds,
        );
        check_call_fail(
            &mut rt,
            power::Method::CreateMiner as u64,
            &constructor_params,
            ExitCode::ErrInsufficientFunds,
        );
    }
}

mod test_update_claimed_power_failures {
    use super::*;
    // Implements the "fails if caller is not a StorageMinerActor" and "fails if claim does not exist for caller" tests from lotus
    #[test]
    fn caller_checks() {
        let actor_error_pairs: [(Cid, ExitCode); 2] = [
            (SYSTEM_ACTOR_CODE_ID.clone(), ExitCode::SysErrForbidden),
            (MINER_ACTOR_CODE_ID.clone(), ExitCode::ErrNotFound),
        ];

        let params = Serialized::serialize(power::UpdateClaimedPowerParams {
            raw_byte_delta: StoragePower::from(100),
            quality_adjusted_delta: StoragePower::from(200),
        })
        .unwrap();

        for (actor, exit_code) in &actor_error_pairs {
            let mut rt = construct_and_verify();
            rt.set_caller(actor.to_owned(), MINER_1.clone());
            rt.expect_validate_caller_type(vec![MINER_ACTOR_CODE_ID.clone()]);
            check_call_fail(
                &mut rt,
                power::Method::UpdateClaimedPower as u64,
                &params,
                exit_code.to_owned(),
            );
            rt.verify();
        }
    }
}

mod test_enroll_cron_epoch {
    use super::*;
    #[test]
    fn fails_if_epoch_is_negative() {
        let mut rt = construct_and_verify();
        assert_eq!(
            ExitCode::ErrIllegalArgument,
            enroll_cron_event(
                &mut rt,
                MINER_1.clone(),
                -1,
                Serialized::serialize("payload".as_bytes()).unwrap()
            )
            .unwrap_err()
            .exit_code(),
        );
    }

    #[test]
    fn enroll_for_an_epoch_before_the_current_epoch() {
        let mut rt = construct_and_verify();
        rt.epoch = 5;
        for i in 0..2 {
            let mut p = "hello".as_bytes().to_vec();
            p.extend(i.to_string().as_bytes());
            let payload = Serialized::serialize(p).unwrap();
            let e = 2 - i;
            assert!(enroll_cron_event(&mut rt, MINER_1.clone(), e, payload.clone()).is_ok());
            let events = get_enrolled_cron_ticks(&mut rt, e);
            let evt = &events[0];
            assert_eq!(payload, evt.callback_payload);
            assert_eq!(MINER_1.clone(), evt.miner_addr);
            let state: power::State = rt.get_state().unwrap();
            assert_eq!(0, state.first_cron_epoch);
        }
    }
    #[test]
    fn enroll_multiple_events() {
        let mut rt = construct_and_verify();
        let ps = ["hello", "hello2", "test"];
        let actions = [
            (1, 1, Some(MINER_1.clone())),
            (1, 2, None),
            (2, 1, Some(MINER_2.clone())),
        ];
        let mut actor_seed = 1;
        let mut miner = MINER_1.clone();

        for (index, (event_epoch, num_events_to_check, create_miner)) in actions.iter().enumerate()
        {
            if let Some(miner_to_create) = create_miner {
                actor_seed = create_miner_basic(
                    &mut rt,
                    OWNER.clone(),
                    OWNER.clone(),
                    miner_to_create.clone(),
                    actor_seed,
                );
                miner = miner_to_create.clone();
            }
            let payload = Serialized::serialize(ps[index].as_bytes()).unwrap();
            assert!(enroll_cron_event(&mut rt, miner, *event_epoch, payload.clone()).is_ok());
            let events = get_enrolled_cron_ticks(&mut rt, *event_epoch);
            for i in 0..*num_events_to_check {
                let payload_index = index + i + 1 - num_events_to_check;
                let payload = Serialized::serialize(ps[payload_index].as_bytes()).unwrap();
                let evt = &events[i as usize];
                assert_eq!(payload, evt.callback_payload);
                assert_eq!(miner, evt.miner_addr);
            }
        }
    }
}

mod test_power_and_pledge_accounting {
    use super::*;

    #[test]
    fn power_and_pledge_accounted_below_threshold() {
        let mut rt = construct_and_verify();
        let mut actor_seed = 1;
        for miner in &[MINER_1.clone(), MINER_2.clone()] {
            actor_seed =
                create_miner_basic(&mut rt, OWNER.clone(), OWNER.clone(), *miner, actor_seed);
        }

        let ret = current_total_power(&mut rt).unwrap();
        assert_eq!(StoragePower::default(), ret.raw_byte_power);
        assert_eq!(StoragePower::default(), ret.quality_adj_power);
        assert_eq!(StoragePower::default(), ret.pledge_collateral);

        let mut total_raw_multi = 0;
        let mut total_qa_multi = 0;
        let mut pledge_total = StoragePower::default();

        let mut update_and_expect =
            |rt: &mut MockRuntime,
             miner: Address,
             raw_multi: i64,
             qa_multi: i64,
             pledge_total_delta: StoragePower| {
                // Updates and expects power
                update_claimed_power(
                    rt,
                    miner,
                    SMALL_POWER_UNIT.clone() * raw_multi,
                    SMALL_POWER_UNIT.clone() * qa_multi,
                )
                .unwrap();
                total_raw_multi += raw_multi;
                total_qa_multi += qa_multi;

                expect_total_power_eager(
                    rt,
                    SMALL_POWER_UNIT.clone() * total_raw_multi,
                    SMALL_POWER_UNIT.clone() * total_qa_multi,
                );

                //Updates and expects pledges
                pledge_total += &pledge_total_delta;
                update_pledge_total(rt, miner, pledge_total_delta).unwrap();
                expect_total_pledge_eager(rt, pledge_total.clone());
            };

        update_and_expect(&mut rt, MINER_1.clone(), 1, 2, StoragePower::default());
        update_and_expect(
            &mut rt,
            MINER_2.clone(),
            1,
            1,
            StoragePower::from(1_000_000),
        );

        rt.verify();

        let cl = get_claim(&mut rt, MINER_1.clone());
        assert_eq!(SMALL_POWER_UNIT.clone(), cl.raw_byte_power);
        assert_eq!(SMALL_POWER_UNIT.clone() * 2, cl.quality_adj_power);
        let cl = get_claim(&mut rt, MINER_2.clone());
        assert_eq!(SMALL_POWER_UNIT.clone(), cl.raw_byte_power);
        assert_eq!(SMALL_POWER_UNIT.clone(), cl.quality_adj_power);

        update_and_expect(
            &mut rt,
            MINER_2.clone(),
            -1,
            -1,
            -1 * StoragePower::from(100_000),
        );

        let cl = get_claim(&mut rt, MINER_2.clone());
        assert_eq!(TokenAmount::default(), cl.raw_byte_power);
        assert_eq!(TokenAmount::default(), cl.quality_adj_power);
    }

    #[test]
    fn power_accounting_crossing_threshold() {
        let mut rt = construct_and_verify();
        let power_units = [
            SMALL_POWER_UNIT.clone(),
            SMALL_POWER_UNIT.clone(),
            POWER_UNIT.clone(),
            POWER_UNIT.clone(),
            POWER_UNIT.clone(),
        ];
        let mut miner_id = MINER_1_ID;
        let mut actor_seed = 1;

        for power_unit in &power_units {
            let miner_addr = Address::new_id(miner_id);
            actor_seed = create_miner_basic(
                &mut rt,
                OWNER.clone(),
                OWNER.clone(),
                miner_addr,
                actor_seed,
            );
            miner_id += 1;
            // Use qa power 10x raw power to show it's not being used for threshold calculations.
            update_claimed_power(&mut rt, miner_addr, power_unit.clone(), power_unit * 10).unwrap()
        }

        // Below threshold small miner power is counted
        let expected_total_below: StoragePower =
            SMALL_POWER_UNIT.clone() * 2 + POWER_UNIT.clone() * 3;
        expect_total_power_eager(
            &mut rt,
            expected_total_below.clone(),
            expected_total_below.clone() * 10,
        );

        // Above threshold (power.ConsensusMinerMinMiners = 4) small miner power is ignored
        let delta = POWER_UNIT.clone() - SMALL_POWER_UNIT.clone();
        let miner_2 = Address::new_id(MINER_1_ID + 1);
        assert!(update_claimed_power(&mut rt, miner_2, delta.clone(), delta.clone() * 10).is_ok());
        let expected_total_above: StoragePower = POWER_UNIT.clone() * 4;
        expect_total_power_eager(
            &mut rt,
            expected_total_above.clone(),
            expected_total_above * 10,
        );

        let state: power::State = rt.get_state().unwrap();
        assert_eq!(4, state.miner_above_min_power_count);

        // Less than 4 miners above threshold again small miner power is counted again
        let miner_4 = Address::new_id(MINER_1_ID + 3);
        assert!(update_claimed_power(&mut rt, miner_4, -1 * delta.clone(), delta * -10).is_ok());
        expect_total_power_eager(
            &mut rt,
            expected_total_below.clone(),
            expected_total_below * 10,
        );
    }

    #[test]
    fn miner_power_disappear_once_below_power_threshold(){
        let mut rt = construct_and_verify();
        let mut actor_seed = 1;
        let mut miner_id = MINER_1_ID;
        for _ in 0..5 {
            let miner_addr = Address::new_id(miner_id);
            actor_seed = create_miner_basic(&mut rt, OWNER.clone(), OWNER.clone(), miner_addr, actor_seed);
            update_claimed_power(&mut rt, miner_addr, POWER_UNIT.clone(), POWER_UNIT.clone()).unwrap();
            miner_id += 1;
        }
        
        let expected_total: StoragePower = POWER_UNIT.clone() * 5;
        expect_total_power_eager(&mut rt, expected_total.clone(), expected_total);

        let miner_4 = Address::new_id(MINER_1_ID + 3);
        update_claimed_power(&mut rt, miner_4, SMALL_POWER_UNIT.clone() * -1, SMALL_POWER_UNIT.clone() * -1).unwrap();
        
		let expected_total: StoragePower = POWER_UNIT.clone() * 4;
        expect_total_power_eager(&mut rt, expected_total.clone(), expected_total.clone());
    }
    
    #[test]
    fn threshold_only_depends_on_raw_power() {
        let mut rt = construct_and_verify();
        let mut actor_seed = 1;
        for i in 0..4 {
            let miner_addr = Address::new_id(MINER_1_ID + i);
            actor_seed = create_miner_basic(&mut rt, OWNER.clone(), OWNER.clone(), miner_addr, actor_seed);
        }

        for i in 0..2 {
            for j in 0..3 {
                let miner_addr = Address::new_id(MINER_1_ID + j);
                update_claimed_power(&mut rt, miner_addr, POWER_UNIT.clone()/2, POWER_UNIT.clone()).unwrap()
            }
            let state : power::State = rt.get_state().unwrap();
            assert_eq!(3*i,state.miner_above_min_power_count);
        }
    }
    
    #[test]
    fn qa_power_is_above_threshold_before_and_after_update(){
        let mut rt = construct_and_verify();
        create_miner_basic(&mut rt, OWNER.clone(), OWNER.clone(), MINER_1.clone(), 1);
        let mut total = 0;

        for i in &[3,1]{
            update_claimed_power(&mut rt, MINER_1.clone(), i * POWER_UNIT.clone(), i * POWER_UNIT.clone()).unwrap();
            total +=i;
            let state : power::State = rt.get_state().unwrap();
            assert_eq!(total* POWER_UNIT.clone(),state.total_quality_adj_power);
            assert_eq!(total* POWER_UNIT.clone(),state.total_raw_byte_power);
        }
	}

    #[test]
	fn claimed_power_is_externally_available(){
        let mut rt = construct_and_verify();
        create_miner_basic(&mut rt, OWNER.clone(), OWNER.clone(), MINER_1.clone(), 1);
        update_claimed_power(&mut rt, MINER_1.clone(),  POWER_UNIT.clone(), POWER_UNIT.clone()).unwrap();
        let claim = get_claim(&mut rt, MINER_1.clone());
        assert_eq!(POWER_UNIT.clone(), claim.raw_byte_power);
        assert_eq!(POWER_UNIT.clone(), claim.quality_adj_power);
	}
}

mod test_update_pledge_total {
    use super::*;

    #[test]
	fn update_pledge_total_aborts_if_miner_has_no_claim(){
        let mut rt = construct_and_verify();
        create_miner_basic(&mut rt, OWNER.clone(), OWNER.clone(), MINER_1.clone(), 1);
        delete_claim(&mut rt, MINER_1.clone());
        assert_eq!(
            ExitCode::ErrForbidden,
            update_pledge_total(&mut rt, MINER_1.clone(), TokenAmount::from(1_000_000)).unwrap_err().exit_code());
	}
}


mod test_cron{
    use super::*;
    #[test]
    fn calls_reward_actor(){
        let mut rt = construct_and_verify();
        let expected_power = StoragePower::default();
        rt.epoch = 1;
        rt.expect_validate_caller_addr(vec![CRON_ACTOR_ADDR.clone()]);
        let params = Serialized::serialize(BigIntSer(&expected_power) ).unwrap();
        rt.expect_send(REWARD_ACTOR_ADDR.clone(), reward::Method::UpdateNetworkKPI as u64, params, TokenAmount::default(), Serialized::default(), ExitCode::Ok);
        rt.set_caller(CRON_ACTOR_CODE_ID.clone() , CRON_ACTOR_ADDR.clone());

        //TODO add expect batch verify seals		

        assert!(call(&mut rt, power::Method::OnEpochTickEnd as u64, &Serialized::default()).is_ok());
		rt.verify()
    }

    #[test]
    fn test_amount_sent_to_reward_actor_and_state_change() {
		// powerUnit, err := builtin.ConsensusMinerMinPower(abi.RegisteredSealProof_StackedDrg2KiBV1)
		// require.NoError(t, err)

		// miner3 := tutil.NewIDAddr(t, 103)
		// miner4 := tutil.NewIDAddr(t, 104)

		// rt := builder.Build(t)
		// actor.constructAndVerify(rt)

		// actor.createMinerBasic(rt, owner, owner, miner1)
		// actor.createMinerBasic(rt, owner, owner, miner2)
		// actor.createMinerBasic(rt, owner, owner, miner3)
		// actor.createMinerBasic(rt, owner, owner, miner4)
		// actor.updateClaimedPower(rt, miner1, powerUnit, powerUnit)
		// actor.updateClaimedPower(rt, miner1, powerUnit, powerUnit)
		// actor.updateClaimedPower(rt, miner1, powerUnit, powerUnit)
		// actor.updateClaimedPower(rt, miner1, powerUnit, powerUnit)

		// expectedPower := big.Mul(big.NewInt(4), powerUnit)

		// delta := abi.NewTokenAmount(1)
		// actor.updatePledgeTotal(rt, miner1, delta)
		// actor.onEpochTickEnd(rt, 0, expectedPower, nil, nil)

		// st := getState(rt)
		// require.EqualValues(t, delta, st.ThisEpochPledgeCollateral)
		// require.EqualValues(t, expectedPower, st.ThisEpochQualityAdjPower)
		// require.EqualValues(t, expectedPower, st.ThisEpochRawBytePower)
	}
    #[test]
    fn fails_to_enroll_if_epoch_is_negative(){
        let mut rt = construct_and_verify();
        assert_eq!(ExitCode::ErrIllegalArgument,
            enroll_cron_event(&mut rt, MINER_1.clone(), -2 , Serialized::serialize(vec![1,3]).unwrap()).unwrap_err().exit_code()
         );
	}

}



fn check_call_fail(rt: &mut MockRuntime, method_num: u64, ser: &Serialized, exit_code: ExitCode) {
    assert_eq!(
        exit_code,
        call(rt, method_num, &ser).unwrap_err().exit_code()
    );
}

fn call(rt: &mut MockRuntime, method_num: u64, ser: &Serialized) -> Result<Serialized, ActorError> {
    rt.call(&*POWER_ACTOR_CODE_ID, method_num, ser)
}

fn construct_and_verify() -> MockRuntime {
    let mut rt = MockRuntime {
        receiver: *STORAGE_POWER_ACTOR_ADDR,
        caller: *SYSTEM_ACTOR_ADDR,
        caller_type: SYSTEM_ACTOR_CODE_ID.clone(),
        ..Default::default()
    };

    rt.expect_validate_caller_addr(vec![SYSTEM_ACTOR_ADDR.clone()]);

    assert_eq!(
        Serialized::default(),
        call(&mut rt, METHOD_CONSTRUCTOR, &Serialized::default()).unwrap()
    );

    rt.verify();
    let state: power::State = rt.get_state().unwrap();
    let zero = StoragePower::default();
    assert_eq!(zero, state.total_raw_byte_power);
    assert_eq!(zero, state.total_bytes_committed);
    assert_eq!(zero, state.total_quality_adj_power);
    assert_eq!(zero, state.total_qa_bytes_committed);
    assert_eq!(zero, state.total_pledge_collateral);
    assert_eq!(zero, state.this_epoch_raw_byte_power);
    assert_eq!(zero, state.this_epoch_quality_adj_power);
    assert_eq!(zero, state.this_epoch_pledge_collateral);
    assert_eq!(0, state.first_cron_epoch);
    assert_eq!(0, state.miner_count);
    assert_eq!(0, state.miner_above_min_power_count);
    verify_cron_size(&state.cron_event_queue, &rt.store, 0);
    verify_claim_size(&state.claims, &rt.store, 0);
    rt
}

fn create_miner(
    rt: &mut MockRuntime,
    owner: Address,
    worker: Address,
    miner: Address,
    robust: Address,
    multiaddrs: Vec<BytesDe>,
    seal_proof_type: RegisteredSealProof,
    value: TokenAmount,
    peer: BytesDe,
) -> Serialized {
    let creater_params = power::CreateMinerParams {
        owner,
        worker,
        control_addresses: vec![],
        seal_proof_type,
        multiaddrs,
        peer,
    };

    let state: power::State = rt.get_state().unwrap();
    let prev_miner_count = state.miner_count;

    rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), creater_params.owner);
    rt.set_value(value.clone());
    rt.set_balance(value.clone());
    rt.expect_validate_caller_type(vec![
        ACCOUNT_ACTOR_CODE_ID.clone(),
        MULTISIG_ACTOR_CODE_ID.clone(),
    ]);

    let miner_ret = power::CreateMinerReturn {
        id_address: miner,
        robust_address: robust,
    };
    let send_return = Serialized::serialize(miner_ret).unwrap();
    let create_params = Serialized::serialize(creater_params).unwrap();

    let exec_params = init::ExecParams {
        code_cid: MINER_ACTOR_CODE_ID.clone(),
        constructor_params: create_params.clone(),
    };
    let msg_params = Serialized::serialize(exec_params).unwrap();

    rt.expect_send(
        *INIT_ACTOR_ADDR,
        init::Method::Exec as u64,
        msg_params,
        value,
        send_return,
        ExitCode::Ok,
    );

    assert!(call(rt, power::Method::CreateMiner as u64, &create_params).is_ok());
    rt.verify();
    let cl = get_claim(rt, miner);

    assert_eq!(StoragePower::default(), cl.raw_byte_power);
    assert_eq!(StoragePower::default(), cl.quality_adj_power);
    let state: power::State = rt.get_state().unwrap();
    assert_eq!(prev_miner_count + 1, state.miner_count);

    Serialized::serialize(create_params).unwrap()
}

fn create_miner_basic(
    rt: &mut MockRuntime,
    owner: Address,
    worker: Address,
    miner: Address,
    actor_seed: u64,
) -> u64 {
    let string = actor_seed.to_string();
    let actr_addr = Address::new_actor(string.as_bytes());

    create_miner(
        rt,
        owner,
        worker,
        miner,
        actr_addr,
        vec![],
        RegisteredSealProof::StackedDRG32GiBV1,
        TokenAmount::default(),
        BytesDe(string.as_bytes().to_vec()),
    );
    actor_seed + 1
}

fn init_create_miner_bytes(
    owner: Address,
    worker: Address,
    peer: BytesDe,
    multiaddrs: Vec<BytesDe>,
    seal_proof_type: RegisteredSealProof,
) -> Serialized {
    let v = power::CreateMinerParams {
        owner,
        worker,
        peer,
        multiaddrs,
        seal_proof_type,
        control_addresses: vec![],
    };
    Serialized::serialize(v).unwrap()
}

fn current_total_power(rt: &mut MockRuntime) -> Result<power::CurrentTotalPowerReturn, ActorError> {
    rt.expect_validate_caller_any();
    let ser = call(
        rt,
        power::Method::CurrentTotalPower as u64,
        &Serialized::default(),
    )?;
    rt.verify();
    Ok(Serialized::deserialize(&ser).unwrap())
}
fn expect_total_power_eager(
    rt: &mut MockRuntime,
    expected_raw: StoragePower,
    expected_qa: StoragePower,
) {
    let state: power::State = rt.get_state().unwrap();
    let (total_raw_byte_power, total_quality_adj_power) = state.current_total_power();
    println!("total_raw_byte_power is {:?}",total_raw_byte_power);
    println!("total_quality_adj_power is {:?}",total_quality_adj_power);
    println!("expected_raw_byte_power is {:?}",expected_raw);
    println!("expected_quality_adj_power is {:?}",expected_qa);
    assert_eq!(expected_raw, total_raw_byte_power);
    assert_eq!(expected_qa, total_quality_adj_power);
}

fn expect_total_pledge_eager(rt: &mut MockRuntime, expected_pledge: TokenAmount) {
    let state: power::State = rt.get_state().unwrap();
    assert_eq!(expected_pledge, state.total_pledge_collateral);
}

fn update_claimed_power(
    rt: &mut MockRuntime,
    miner: Address,
    raw_delta: StoragePower,
    qa_delta: StoragePower,
) -> Result<(), ActorError> {
    let prev_cl = get_claim(rt, miner);
    let params = power::UpdateClaimedPowerParams {
        raw_byte_delta: raw_delta.clone(),
        quality_adjusted_delta: qa_delta.clone(),
    };
    rt.set_caller(MINER_ACTOR_CODE_ID.clone(), miner);
    rt.expect_validate_caller_type(vec![MINER_ACTOR_CODE_ID.clone()]);
    call(
        rt,
        power::Method::UpdateClaimedPower as u64,
        &Serialized::serialize(params)?,
    )?;
    rt.verify();
    let cl = get_claim(rt, miner);
    let expected_raw = prev_cl.raw_byte_power + &raw_delta;
    let expected_adjusted = prev_cl.quality_adj_power + &qa_delta;

    if expected_raw == StoragePower::default() {
        assert_eq!(StoragePower::default(), cl.raw_byte_power);
    } else {
        assert_eq!(expected_raw, cl.raw_byte_power);
    }
    if expected_adjusted == StoragePower::default() {
        assert_eq!(StoragePower::default(), cl.quality_adj_power);
    } else {
        assert_eq!(expected_adjusted, cl.quality_adj_power);
    }
    Ok(())
}

fn update_pledge_total(
    rt: &mut MockRuntime,
    miner: Address,
    delta: TokenAmount,
) -> Result<(), ActorError> {
    let state: power::State = rt.get_state().unwrap();
    let prev = state.total_pledge_collateral;
    rt.set_caller(MINER_ACTOR_CODE_ID.clone(), miner);
    rt.expect_validate_caller_type(vec![MINER_ACTOR_CODE_ID.clone()]);
    call(
        rt,
        power::Method::UpdatePledgeTotal as u64,
        &Serialized::serialize(BigIntSer(&delta)).unwrap(),
    )?;
    rt.verify();
    let state: power::State = rt.get_state().unwrap();
    assert_eq!(prev + delta, state.total_pledge_collateral);
    Ok(())
}

fn get_claim(rt: &mut MockRuntime, a: Address) -> power::Claim {
    let state: power::State = rt.get_state().unwrap();

    let claims = make_map_with_root(&state.claims, &rt.store).unwrap();

    claims.get(&a.to_bytes()).unwrap().unwrap()
}

fn delete_claim(rt : &mut MockRuntime, a : Address){
    let mut state: power::State = rt.get_state().unwrap();
    let mut claims: Hamt<_, power::Claim> = make_map_with_root(&state.claims, &rt.store).unwrap();
    claims.delete(&a.to_bytes()).unwrap();
    state.claims = claims.flush().unwrap();
    rt.replace_state(&state);

}

fn enroll_cron_event(
    rt: &mut MockRuntime,
    miner: Address,
    event_epoch: ChainEpoch,
    payload: Serialized,
) -> Result<Serialized, ActorError> {
    rt.expect_validate_caller_type(vec![MINER_ACTOR_CODE_ID.clone()]);
    rt.set_caller(MINER_ACTOR_CODE_ID.clone(), miner.clone());
    let params = power::EnrollCronEventParams {
        event_epoch,
        payload,
    };
    let serialized = call(
        rt,
        power::Method::EnrollCronEvent as u64,
        &Serialized::serialize(params).unwrap(),
    )?;
    rt.verify();

    Ok(serialized)
}

fn get_enrolled_cron_ticks(rt: &mut MockRuntime, epoch: ChainEpoch) -> Vec<power::CronEvent> {
    let state: power::State = rt.get_state().unwrap();
    let events = Multimap::from_root(&rt.store, &state.cron_event_queue).unwrap();
    power::load_cron_events(&events, epoch).unwrap()
}

pub fn collect_claim_keys<BS: BlockStore>(
    root: &Cid,
    store: &BS,
) -> Result<Vec<BytesKey>, ActorError> {
    let mut ret_keys = Vec::new();
    let claims: Hamt<BS, power::Claim> = make_map_with_root(&root, store).unwrap();
    claims
        .for_each(|k: &BytesKey, _| {
            ret_keys.push(k.clone());
            Ok(())
        })
        .unwrap();
    Ok(ret_keys)
}
pub fn collect_cron_keys<BS: BlockStore>(
    root: &Cid,
    store: &BS,
) -> Result<Vec<BytesKey>, ActorError> {
    let mut ret_keys = Vec::new();
    let crons: Hamt<BS, power::CronEvent> = make_map_with_root(&root, store).unwrap();
    crons
        .for_each(|k: &BytesKey, _| {
            ret_keys.push(k.clone());
            Ok(())
        })
        .unwrap();
    Ok(ret_keys)
}

pub fn verify_map_size(keys: Vec<BytesKey>, size: usize) {
    assert_eq!(size, keys.len());
}

pub fn verify_cron_size<BS: BlockStore>(root: &Cid, store: &BS, size: usize) {
    let cron = collect_cron_keys(&root, store).unwrap();
    verify_map_size(cron, size);
}
pub fn verify_claim_size<BS: BlockStore>(root: &Cid, store: &BS, size: usize) {
    let cron = collect_claim_keys(&root, store).unwrap();
    verify_map_size(cron, size);
}
