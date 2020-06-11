// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod common;

use actor::{
    multisig::{
        ConstructorParams, Method, ProposalHashData, ProposeParams, State, Transaction, TxnID,
        TxnIDParams,
    },
    Multimap, ACCOUNT_ACTOR_CODE_ID, INIT_ACTOR_ADDR, INIT_ACTOR_CODE_ID, MULTISIG_ACTOR_CODE_ID,
    SYSTEM_ACTOR_ADDR, SYSTEM_ACTOR_CODE_ID,
};
use address::Address;
use clock::ChainEpoch;
use common::*;
use db::MemoryDB;
use encoding::blake2b_256;
use ipld_blockstore::BlockStore;
use message::UnsignedMessage;
use vm::{ActorError, ExitCode, Serialized, TokenAmount, METHOD_SEND};

enum TestId {
    Receiver = 100,
    Anne = 101,
    Bob = 102,
    Charlie = 103,
}

fn construct_and_verify<'a, BS: BlockStore>(
    rt: &mut MockRuntime<'a, BS>,
    signers: Vec<Address>,
    num_approvals_threshold: i64,
    unlock_duration: ChainEpoch,
) {
    let params = ConstructorParams {
        signers: signers,
        num_approvals_threshold: num_approvals_threshold,
        unlock_duration: unlock_duration,
    };

    rt.expect_validate_caller_addr(&[*INIT_ACTOR_ADDR]);
    assert!(rt
        .call(
            &*MULTISIG_ACTOR_CODE_ID,
            Method::Constructor as u64,
            &Serialized::serialize(&params).unwrap()
        )
        .is_ok());
    rt.verify();
}

fn propose<'a, BS: BlockStore>(
    rt: &mut MockRuntime<'a, BS>,
    to: Address,
    value: TokenAmount,
    method: u64,
    params: Serialized,
) -> Result<Serialized, ActorError>{
    let call_params = ProposeParams {
        to,
        value,
        method,
        params,
    };
    rt
        .call(
            &*MULTISIG_ACTOR_CODE_ID,
            Method::Propose as u64,
            &Serialized::serialize(&call_params).unwrap()
        )
}

fn approve<'a, BS: BlockStore>(rt: &mut MockRuntime<'a, BS>, txn_id: i64, params: [u8; 32]) -> Result<Serialized, ActorError> {
    let params = TxnIDParams {
        id: TxnID(txn_id),
        proposal_hash: params,
    };
    rt
        .call(
            &*MULTISIG_ACTOR_CODE_ID,
            Method::Approve as u64,
            &Serialized::serialize(&params).unwrap()
        )
}

fn make_proposal_hash(
    approved: Vec<Address>,
    to: Address,
    value: TokenAmount,
    method: u64,
    params: &[u8],
) -> [u8; 32] {
    let hash_data = ProposalHashData {
        requester: approved[0],
        to,
        value,
        method,
        params: params.to_vec(),
    };
    let serial_data = Serialized::serialize(hash_data).unwrap();
    blake2b_256(serial_data.bytes())
}

fn  assert_transactions<'a, BS: BlockStore>(rt: &mut MockRuntime<'a, BS>, expected : Vec<Transaction>){
    let state : State = rt.get_state().unwrap();
    //let txns = Multimap::from_root(rt.store, &state.pending_txs).unwrap();
    //txns.

}

mod construction_tests {

    use super::*;
    fn construct_runtime<'a, BS: BlockStore>(bs: &'a BS) -> MockRuntime<'a, BS> {
        let receiver = Address::new_id(TestId::Receiver as u64);
        let message = UnsignedMessage::builder()
            .to(receiver.clone())
            .from(SYSTEM_ACTOR_ADDR.clone())
            .build()
            .unwrap();
        let mut rt = MockRuntime::new(bs, message);
        rt.set_caller(INIT_ACTOR_CODE_ID.clone(), INIT_ACTOR_ADDR.clone());
        return rt;
    }

    #[test]
    fn simple() {
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        let params = ConstructorParams {
            signers: vec![
                Address::new_id(TestId::Anne as u64),
                Address::new_id(TestId::Bob as u64),
                Address::new_id(TestId::Charlie as u64),
            ],
            num_approvals_threshold: 2,
            unlock_duration: 0,
        };

        rt.expect_validate_caller_addr(&[*INIT_ACTOR_ADDR]);
        assert!(rt
            .call(
                &*MULTISIG_ACTOR_CODE_ID,
                Method::Constructor as u64,
                &Serialized::serialize(&params).unwrap()
            )
            .is_ok());
        rt.verify();

        let state: State = rt.get_state().unwrap();
        assert_eq!(params.signers, state.signers);
        assert_eq!(params.signers, state.signers);
        assert_eq!(
            params.num_approvals_threshold,
            state.num_approvals_threshold
        );
        assert_eq!(TokenAmount::from(0u8), state.initial_balance);
        assert_eq!(0, state.unlock_duration);
        assert_eq!(0, state.start_epoch);

        let txns = Multimap::from_root(rt.store, &state.pending_txs).unwrap();

        //TODO
        // keys, err := txns.CollectKeys()
        // require.NoError(t, err)
        // assert.Empty(t, keys)
    }
    #[test]
    fn vesting() {
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        rt.epoch = 1234;
        let params = ConstructorParams {
            signers: vec![
                Address::new_id(TestId::Anne as u64),
                Address::new_id(TestId::Bob as u64),
                Address::new_id(TestId::Charlie as u64),
            ],
            num_approvals_threshold: 3,
            unlock_duration: 100,
        };
        rt.expect_validate_caller_addr(&[*INIT_ACTOR_ADDR]);
        assert!(rt
            .call(
                &*MULTISIG_ACTOR_CODE_ID,
                Method::Constructor as u64,
                &Serialized::serialize(&params).unwrap()
            )
            .is_ok());
        rt.verify();

        let state: State = rt.get_state().unwrap();
        assert_eq!(params.signers, state.signers);
        assert_eq!(params.signers, state.signers);
        assert_eq!(
            params.num_approvals_threshold,
            state.num_approvals_threshold
        );
        assert_eq!(TokenAmount::from(0u8), state.initial_balance);
        assert_eq!(100, state.unlock_duration);
        assert_eq!(1234, state.start_epoch);
    }
    #[test]
    fn zero_signers() {
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        rt.epoch = 1234;
        let params = ConstructorParams {
            signers: vec![],
            num_approvals_threshold: 1,
            unlock_duration: 1,
        };
        rt.expect_validate_caller_addr(&[*INIT_ACTOR_ADDR]);
        let error = rt
            .call(
                &*MULTISIG_ACTOR_CODE_ID,
                Method::Constructor as u64,
                &Serialized::serialize(&params).unwrap(),
            )
            .unwrap_err();
        assert_eq!(error.exit_code(), ExitCode::ErrIllegalArgument);
        rt.verify();
    }
}

mod test_vesting {
    use super::*;
    const UNLOCK_DURATION: u64 = 10;
    const INITIAL_BALANCE: u64 = 100;
    const DARLENE : u64 = 103;

    fn construct_runtime<'a, BS: BlockStore>(bs: &'a BS) -> MockRuntime<'a, BS> {
        let receiver = Address::new_id(TestId::Receiver as u64);
        let initial_balance = TokenAmount::from(INITIAL_BALANCE);
        let message = UnsignedMessage::builder()
            .to(receiver.clone())
            .from(SYSTEM_ACTOR_ADDR.clone())
            .build()
            .unwrap();
        let mut rt = MockRuntime::new(bs, message);
        rt.set_caller(INIT_ACTOR_CODE_ID.clone(), INIT_ACTOR_ADDR.clone());
        rt.balance = initial_balance.clone();
        rt.received = initial_balance;
        return rt;
    }

    #[test]
    fn happy_path() {
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        construct_and_verify(
            &mut rt,
            vec![
                Address::new_id(TestId::Anne as u64),
                Address::new_id(TestId::Bob as u64),
                Address::new_id(TestId::Charlie as u64),
            ],
            2,
            UNLOCK_DURATION,
        );
        let anne = Address::new_id(TestId::Anne as u64);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), anne.clone());
        rt.received = TokenAmount::from(0u8);
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let darlene = Address::new_id(DARLENE);
        let initial_balance = TokenAmount::from(INITIAL_BALANCE);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        assert!(propose(
            &mut rt,
            darlene,
            initial_balance.clone(),
            METHOD_SEND,
            fake_params.clone(),
        ).is_ok());
        rt.verify();
        rt.epoch = UNLOCK_DURATION;
        rt.set_caller(
            ACCOUNT_ACTOR_CODE_ID.clone(),
            Address::new_id(TestId::Bob as u64),
        );
        rt.expect_send(
            darlene.clone(),
            METHOD_SEND,
            fake_params.clone(),
            initial_balance.clone(),
            Serialized::default(),
            ExitCode::Ok,
        );
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let proposal_hash_data = make_proposal_hash(
            vec![Address::new_id(TestId::Anne as u64)],
            darlene,
            initial_balance,
            METHOD_SEND,
            fake_params.bytes(),
        );
        assert!(approve(&mut rt, 0, proposal_hash_data).is_ok());
        rt.verify();
    }

    #[test]
    fn partial_vesting(){
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        construct_and_verify(
            &mut rt,
            vec![
                Address::new_id(TestId::Anne as u64),
                Address::new_id(TestId::Bob as u64),
                Address::new_id(TestId::Charlie as u64),
            ],
            2,
            UNLOCK_DURATION,
        );
        let anne = Address::new_id(TestId::Anne as u64);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), anne.clone());
        rt.received = TokenAmount::from(0u8);
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let darlene = Address::new_id(DARLENE);
        let half_initial_balance = TokenAmount::from(INITIAL_BALANCE/2);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        assert!(propose(
            &mut rt,
            darlene,
            half_initial_balance.clone(),
            METHOD_SEND,
            fake_params.clone(),
        ).is_ok());
        rt.verify();
        rt.epoch = UNLOCK_DURATION/2;
        rt.set_caller(
            ACCOUNT_ACTOR_CODE_ID.clone(),
            Address::new_id(TestId::Bob as u64),
        );
        rt.expect_send(
            darlene.clone(),
            METHOD_SEND,
            fake_params.clone(),
            half_initial_balance.clone(),
            Serialized::default(),
            ExitCode::Ok,
        );
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let proposal_hash_data = make_proposal_hash(
            vec![Address::new_id(TestId::Anne as u64)],
            darlene,
            half_initial_balance,
            METHOD_SEND,
            fake_params.bytes(),
        );
        assert!(approve(&mut rt, 0, proposal_hash_data).is_ok());
        rt.verify();

    }

    #[test]
    fn auto_approve_above_locked_fail(){
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        construct_and_verify(
            &mut rt,
            vec![
                Address::new_id(TestId::Anne as u64),
                Address::new_id(TestId::Bob as u64),
                Address::new_id(TestId::Charlie as u64),
            ],
            1,
            UNLOCK_DURATION,
        );
        let anne = Address::new_id(TestId::Anne as u64);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), anne.clone());
        rt.received = TokenAmount::from(0u8);
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        let darlene = Address::new_id(DARLENE);
        let error = propose(&mut rt, darlene.clone(),  TokenAmount::from(100u8), METHOD_SEND, fake_params.clone()).unwrap_err();
        assert_eq!(error.exit_code(), ExitCode::ErrInsufficientFunds);
        rt.verify();
        rt.epoch = 1;
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), anne.clone());
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        rt.expect_send(darlene.clone(), METHOD_SEND, fake_params.clone(), TokenAmount::from(10u8), Serialized::default(), ExitCode::Ok);
        assert!(propose(&mut rt, darlene, TokenAmount::from(10u8), METHOD_SEND, fake_params).is_ok());
        rt.verify();
    }

    #[test]
    fn more_than_locked(){
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        construct_and_verify(
            &mut rt,
            vec![
                Address::new_id(TestId::Anne as u64),
                Address::new_id(TestId::Bob as u64),
                Address::new_id(TestId::Charlie as u64),
            ],
            1,
            UNLOCK_DURATION,
        );
        let anne = Address::new_id(TestId::Anne as u64);
        rt.received = TokenAmount::from(0u8);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), anne.clone());
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        let darlene = Address::new_id(DARLENE);
        let tk_amount = TokenAmount::from(INITIAL_BALANCE /2);
        assert!(propose(&mut rt, darlene.clone(),  tk_amount.clone(), METHOD_SEND, fake_params.clone()).is_ok());
        rt.verify();
        rt.epoch = 1;
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(TestId::Bob as u64));
        let proposal_hashed_data = make_proposal_hash(vec![anne.clone()], darlene.clone(), tk_amount.clone(), METHOD_SEND, &fake_params.clone());
        assert_eq!(approve(&mut rt , 0, proposal_hashed_data).unwrap_err().exit_code(), ExitCode::ErrInsufficientFunds);
        rt.verify();
    }

}

mod test_propose{
    use super::*;
    const SEND_VALUE : u64 = 10;
    const NO_LOCK_DUR : u64 = 0;
    const CHUCK : u64 = 103;
    fn construct_runtime<'a, BS: BlockStore>(bs: &'a BS) -> MockRuntime<'a, BS> {
        let receiver = Address::new_id(TestId::Receiver as u64);
        let message = UnsignedMessage::builder()
            .to(receiver.clone())
            .from(SYSTEM_ACTOR_ADDR.clone())
            .build()
            .unwrap();
        let mut rt = MockRuntime::new(bs, message);
        rt.set_caller(INIT_ACTOR_CODE_ID.clone(), INIT_ACTOR_ADDR.clone());
        return rt;
    }

    #[test]
    fn simple(){
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        let num_approvals = 2;
        let signers = vec![Address::new_id(TestId::Anne as u64),Address::new_id(TestId::Bob as u64 )];
        construct_and_verify(&mut rt, signers, num_approvals, NO_LOCK_DUR);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(TestId::Anne as u64));
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        assert!(propose(&mut rt, Address::new_id(CHUCK), TokenAmount::from(SEND_VALUE), METHOD_SEND, fake_params).is_ok());


    }

}