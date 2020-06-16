// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod common;

use actor::{
    multisig::{
        AddSignerParams, ChangeNumApprovalsThresholdParams, ConstructorParams, Method,
        ProposalHashData, ProposeParams, RemoveSignerParams, State, SwapSignerParams, Transaction,
        TxnID, TxnIDParams,
    },
    Multimap, Set, ACCOUNT_ACTOR_CODE_ID, INIT_ACTOR_ADDR, INIT_ACTOR_CODE_ID,
    MULTISIG_ACTOR_CODE_ID, SYSTEM_ACTOR_ADDR, SYSTEM_ACTOR_CODE_ID,
};
use address::Address;
use clock::ChainEpoch;
use common::*;
use db::MemoryDB;
use encoding::blake2b_256;
use ipld_blockstore::BlockStore;
use ipld_hamt::{BytesKey, Hamt};
use message::UnsignedMessage;
use vm::{ActorError, ExitCode, Serialized, TokenAmount, METHOD_SEND};

const RECEIVER: u64 = 100;
const ANNE: u64 = 101;
const BOB: u64 = 102;
const CHARLIE: u64 = 103;

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
) -> Result<Serialized, ActorError> {
    let call_params = ProposeParams {
        to,
        value,
        method,
        params,
    };
    rt.call(
        &*MULTISIG_ACTOR_CODE_ID,
        Method::Propose as u64,
        &Serialized::serialize(&call_params).unwrap(),
    )
}

fn approve<'a, BS: BlockStore>(
    rt: &mut MockRuntime<'a, BS>,
    txn_id: i64,
    params: [u8; 32],
) -> Result<Serialized, ActorError> {
    let params = TxnIDParams {
        id: TxnID(txn_id),
        proposal_hash: params,
    };
    rt.call(
        &*MULTISIG_ACTOR_CODE_ID,
        Method::Approve as u64,
        &Serialized::serialize(&params).unwrap(),
    )
}

fn cancel<'a, BS: BlockStore>(
    rt: &mut MockRuntime<'a, BS>,
    txn_id: i64,
    params: [u8; 32],
) -> Result<Serialized, ActorError> {
    let params = TxnIDParams {
        id: TxnID(txn_id),
        proposal_hash: params,
    };
    rt.call(
        &*MULTISIG_ACTOR_CODE_ID,
        Method::Cancel as u64,
        &Serialized::serialize(&params).unwrap(),
    )
}

fn add_signer<'a, BS: BlockStore>(
    rt: &mut MockRuntime<'a, BS>,
    signer: Address,
    increase: bool,
) -> Result<Serialized, ActorError> {
    let params = AddSignerParams {
        signer: signer,
        increase: increase,
    };
    rt.call(
        &*MULTISIG_ACTOR_CODE_ID,
        Method::AddSigner as u64,
        &Serialized::serialize(&params).unwrap(),
    )
}

fn remove_signer<'a, BS: BlockStore>(
    rt: &mut MockRuntime<'a, BS>,
    signer: Address,
    decrease: bool,
) -> Result<Serialized, ActorError> {
    let params = RemoveSignerParams {
        signer: signer,
        decrease: decrease,
    };
    rt.call(
        &*MULTISIG_ACTOR_CODE_ID,
        Method::AddSigner as u64,
        &Serialized::serialize(&params).unwrap(),
    )
}
fn swap_signers<'a, BS: BlockStore>(
    rt: &mut MockRuntime<'a, BS>,
    old_signer: Address,
    new_signer: Address,
) -> Result<Serialized, ActorError> {
    let params = SwapSignerParams {
        from: old_signer,
        to: new_signer,
    };
    rt.call(
        &*MULTISIG_ACTOR_CODE_ID,
        Method::SwapSigner as u64,
        &Serialized::serialize(&params).unwrap(),
    )
}
fn change_num_approvals_threshold<'a, BS: BlockStore>(
    rt: &mut MockRuntime<'a, BS>,
    new_threshold: i64,
) -> Result<Serialized, ActorError> {
    let params = ChangeNumApprovalsThresholdParams {
        new_threshold: new_threshold,
    };
    rt.call(
        &*MULTISIG_ACTOR_CODE_ID,
        Method::ChangeNumApprovalsThreshold as u64,
        &Serialized::serialize(&params).unwrap(),
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

fn assert_transactions<'a, BS: BlockStore>(
    rt: &mut MockRuntime<'a, BS>,
    expected: Vec<Transaction>,
) {
    let state: State = rt.get_state().unwrap();
    let map: Hamt<BytesKey, _> = Hamt::load(&state.pending_txs, rt.store).unwrap();

    let txns_set = Set::from_root(rt.store, &state.pending_txs).unwrap();
    let txns_multi = Multimap::from_root(rt.store, &state.pending_txs).unwrap();

    let keys = txns_set.collect_keys().unwrap();
    assert_eq!(keys.len(), expected.len());
    let mut count = 0;
    assert!(txns_set
        .for_each(|k| {
            let value: Transaction = map.get(k).unwrap().unwrap();
            assert_eq!(value, expected[count]);
            count += 1;
            Ok(())
        })
        .is_ok());
}

mod construction_tests {

    use super::*;
    fn construct_runtime<'a, BS: BlockStore>(bs: &'a BS) -> MockRuntime<'a, BS> {
        let receiver = Address::new_id(RECEIVER);
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
                Address::new_id(ANNE),
                Address::new_id(BOB),
                Address::new_id(CHARLIE),
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

        let txns = Set::from_root(rt.store, &state.pending_txs)
            .unwrap()
            .collect_keys()
            .unwrap();
        assert_eq!(txns.len(), 0);
    }
    #[test]
    fn vesting() {
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        rt.epoch = 1234;
        let params = ConstructorParams {
            signers: vec![
                Address::new_id(ANNE),
                Address::new_id(BOB),
                Address::new_id(CHARLIE),
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
    const DARLENE: u64 = 103;

    fn construct_runtime<'a, BS: BlockStore>(bs: &'a BS) -> MockRuntime<'a, BS> {
        let receiver = Address::new_id(RECEIVER);
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
                Address::new_id(ANNE),
                Address::new_id(BOB),
                Address::new_id(CHARLIE),
            ],
            2,
            UNLOCK_DURATION,
        );
        let anne = Address::new_id(ANNE);
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
        )
        .is_ok());
        rt.verify();
        rt.epoch = UNLOCK_DURATION;
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(BOB));
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
            vec![Address::new_id(ANNE)],
            darlene,
            initial_balance,
            METHOD_SEND,
            fake_params.bytes(),
        );
        assert!(approve(&mut rt, 0, proposal_hash_data).is_ok());
        rt.verify();
    }

    #[test]
    fn partial_vesting() {
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        construct_and_verify(
            &mut rt,
            vec![
                Address::new_id(ANNE),
                Address::new_id(BOB),
                Address::new_id(CHARLIE),
            ],
            2,
            UNLOCK_DURATION,
        );
        let anne = Address::new_id(ANNE);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), anne.clone());
        rt.received = TokenAmount::from(0u8);
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let darlene = Address::new_id(DARLENE);
        let half_initial_balance = TokenAmount::from(INITIAL_BALANCE / 2);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        assert!(propose(
            &mut rt,
            darlene,
            half_initial_balance.clone(),
            METHOD_SEND,
            fake_params.clone(),
        )
        .is_ok());
        rt.verify();
        rt.epoch = UNLOCK_DURATION / 2;
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(BOB));
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
            vec![Address::new_id(ANNE)],
            darlene,
            half_initial_balance,
            METHOD_SEND,
            fake_params.bytes(),
        );
        assert!(approve(&mut rt, 0, proposal_hash_data).is_ok());
        rt.verify();
    }

    //#[test]
    fn auto_approve_above_locked_fail() {
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        construct_and_verify(
            &mut rt,
            vec![
                Address::new_id(ANNE),
                Address::new_id(BOB),
                Address::new_id(CHARLIE),
            ],
            1,
            UNLOCK_DURATION,
        );
        let anne = Address::new_id(ANNE);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), anne.clone());
        rt.received = TokenAmount::from(0u8);
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        let darlene = Address::new_id(DARLENE);
        let error = propose(
            &mut rt,
            darlene.clone(),
            TokenAmount::from(100u8),
            METHOD_SEND,
            fake_params.clone(),
        )
        .unwrap_err();
        assert_eq!(error.exit_code(), ExitCode::ErrInsufficientFunds);
        rt.verify();
        rt.epoch = 1;
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), anne.clone());
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        rt.expect_send(
            darlene.clone(),
            METHOD_SEND,
            fake_params.clone(),
            TokenAmount::from(10u8),
            Serialized::default(),
            ExitCode::Ok,
        );
        assert!(propose(
            &mut rt,
            darlene,
            TokenAmount::from(10u8),
            METHOD_SEND,
            fake_params
        )
        .is_ok());
        rt.verify();
    }

    //#[test]
    fn more_than_locked() {
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        construct_and_verify(
            &mut rt,
            vec![
                Address::new_id(ANNE),
                Address::new_id(BOB),
                Address::new_id(CHARLIE),
            ],
            2,
            UNLOCK_DURATION,
        );
        rt.received = TokenAmount::from(0u8);
        let anne = Address::new_id(ANNE);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), anne.clone());
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        let darlene = Address::new_id(DARLENE);
        let tk_amount = TokenAmount::from(INITIAL_BALANCE / 2);
        assert!(propose(
            &mut rt,
            darlene.clone(),
            tk_amount.clone(),
            METHOD_SEND,
            fake_params.clone()
        )
        .is_ok());
        rt.verify();
        rt.epoch = 1;
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(BOB));
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let proposal_hashed_data = make_proposal_hash(
            vec![anne.clone()],
            darlene.clone(),
            tk_amount.clone(),
            METHOD_SEND,
            &fake_params.clone(),
        );
        assert_eq!(
            approve(&mut rt, 0, proposal_hashed_data)
                .unwrap_err()
                .exit_code(),
            ExitCode::ErrInsufficientFunds
        );
        rt.verify();
    }
}

mod test_propose {
    use super::*;
    const SEND_VALUE: u64 = 10;
    const NO_LOCK_DUR: u64 = 0;
    const CHUCK: u64 = 103;
    fn construct_runtime<'a, BS: BlockStore>(bs: &'a BS) -> MockRuntime<'a, BS> {
        let receiver = Address::new_id(RECEIVER);
        let message = UnsignedMessage::builder()
            .to(receiver.clone())
            .from(SYSTEM_ACTOR_ADDR.clone())
            .build()
            .unwrap();
        let mut rt = MockRuntime::new(bs, message);
        rt.set_caller(INIT_ACTOR_CODE_ID.clone(), INIT_ACTOR_ADDR.clone());
        return rt;
    }

    //#[test]
    fn simple() {
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        let num_approvals = 2;
        let signers = vec![Address::new_id(ANNE), Address::new_id(BOB)];
        construct_and_verify(&mut rt, signers, num_approvals, NO_LOCK_DUR);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(ANNE));
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        assert!(propose(
            &mut rt,
            Address::new_id(CHUCK),
            TokenAmount::from(SEND_VALUE),
            METHOD_SEND,
            fake_params
        )
        .is_ok());

        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        assert_transactions(
            &mut rt,
            vec![Transaction {
                to: Address::new_id(CHUCK),
                value: TokenAmount::from(SEND_VALUE),
                method: METHOD_SEND,
                params: fake_params,
                approved: vec![Address::new_id(ANNE)],
            }],
        );
    }

    //#[test]
    fn with_threshold_met() {
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        let num_approvals = 1;
        rt.balance = TokenAmount::from(20u8);
        rt.received = TokenAmount::from(0u8);
        let signers = vec![Address::new_id(ANNE), Address::new_id(BOB)];
        construct_and_verify(&mut rt, signers, num_approvals, NO_LOCK_DUR);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(ANNE));
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        assert!(propose(
            &mut rt,
            Address::new_id(CHUCK),
            TokenAmount::from(SEND_VALUE),
            METHOD_SEND,
            fake_params
        )
        .is_ok());
        assert_transactions(&mut rt, vec![]);
        rt.verify();
    }

    #[test]
    fn fail_insufficent_balance() {
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        let num_approvals = 1;
        rt.balance = TokenAmount::from(0u8);
        rt.received = TokenAmount::from(0u8);
        let signers = vec![Address::new_id(ANNE), Address::new_id(BOB)];
        construct_and_verify(&mut rt, signers, num_approvals, NO_LOCK_DUR);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(ANNE));
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        assert_eq!(
            ExitCode::ErrInsufficientFunds,
            propose(
                &mut rt,
                Address::new_id(CHUCK),
                TokenAmount::from(SEND_VALUE),
                METHOD_SEND,
                fake_params
            )
            .unwrap_err()
            .exit_code()
        );

        assert_transactions(&mut rt, vec![]);
        rt.verify();
    }

    #[test]
    fn fail_non_signer() {
        let richard = Address::new_id(105);
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        let num_approvals = 2;
        let signers = vec![Address::new_id(ANNE), Address::new_id(BOB)];
        construct_and_verify(&mut rt, signers, num_approvals, NO_LOCK_DUR);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), richard);
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        assert_eq!(
            ExitCode::ErrForbidden,
            propose(
                &mut rt,
                Address::new_id(CHUCK),
                TokenAmount::from(SEND_VALUE),
                METHOD_SEND,
                fake_params
            )
            .unwrap_err()
            .exit_code()
        );
        assert_transactions(&mut rt, vec![]);
        rt.verify();
    }
}

mod test_approve {
    use super::*;
    const CHUCK: u64 = 103;
    const NO_UNLOCK_DURATION: u64 = 10;
    const NUM_APPROVALS: i64 = 2;
    const TXN_ID: i64 = 0;
    const FAKE_METHOD: u64 = 42;
    const SEND_VALUE: u64 = 10;

    fn construct_runtime<'a, BS: BlockStore>(bs: &'a BS) -> MockRuntime<'a, BS> {
        let receiver = Address::new_id(RECEIVER);
        let message = UnsignedMessage::builder()
            .to(receiver.clone())
            .from(SYSTEM_ACTOR_ADDR.clone())
            .build()
            .unwrap();
        let mut rt = MockRuntime::new(bs, message);
        rt.set_caller(INIT_ACTOR_CODE_ID.clone(), INIT_ACTOR_ADDR.clone());
        return rt;
    }

    //#[test]
    fn simple() {
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        let signers = vec![Address::new_id(ANNE), Address::new_id(BOB)];
        construct_and_verify(&mut rt, signers, NUM_APPROVALS, NO_UNLOCK_DURATION);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(ANNE));
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        let chuck = Address::new_id(CHUCK);
        assert!(propose(
            &mut rt,
            chuck.clone(),
            TokenAmount::from(SEND_VALUE),
            METHOD_SEND,
            fake_params.clone()
        )
        .is_ok());
        rt.verify();
        assert_transactions(
            &mut rt,
            vec![Transaction {
                to: chuck.clone(),
                value: TokenAmount::from(SEND_VALUE),
                method: METHOD_SEND,
                params: fake_params.clone(),
                approved: vec![Address::new_id(ANNE)],
            }],
        );
        rt.balance = TokenAmount::from(SEND_VALUE);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(BOB));
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        rt.expect_send(
            chuck.clone(),
            FAKE_METHOD,
            fake_params.clone(),
            TokenAmount::from(SEND_VALUE),
            Serialized::default(),
            ExitCode::Ok,
        );
        let proposal_hash_data = make_proposal_hash(
            vec![Address::new_id(ANNE)],
            chuck,
            TokenAmount::from(SEND_VALUE),
            FAKE_METHOD,
            fake_params.bytes(),
        );
        assert!(approve(&mut rt, 0, proposal_hash_data).is_ok());
        rt.verify();
        assert_transactions(&mut rt, vec![]);
    }

    //#[test]
    fn fail_with_bad_proposal() {
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        let signers = vec![Address::new_id(ANNE), Address::new_id(BOB)];
        construct_and_verify(&mut rt, signers, NUM_APPROVALS, NO_UNLOCK_DURATION);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(ANNE));
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        let chuck = Address::new_id(CHUCK);
        assert!(propose(
            &mut rt,
            chuck.clone(),
            TokenAmount::from(SEND_VALUE),
            METHOD_SEND,
            fake_params.clone()
        )
        .is_ok());
        rt.verify();
        assert_transactions(
            &mut rt,
            vec![Transaction {
                to: chuck.clone(),
                value: TokenAmount::from(SEND_VALUE),
                method: METHOD_SEND,
                params: fake_params.clone(),
                approved: vec![Address::new_id(ANNE)],
            }],
        );
        rt.balance = TokenAmount::from(SEND_VALUE);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(BOB));
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        rt.expect_send(
            chuck.clone(),
            FAKE_METHOD,
            fake_params.clone(),
            TokenAmount::from(SEND_VALUE),
            Serialized::default(),
            ExitCode::Ok,
        );
        let proposal_hash_data = make_proposal_hash(
            vec![Address::new_id(ANNE)],
            chuck,
            TokenAmount::from(SEND_VALUE),
            FAKE_METHOD,
            fake_params.bytes(),
        );
        assert_eq!(
            ExitCode::ErrIllegalState,
            approve(&mut rt, TXN_ID, proposal_hash_data)
                .unwrap_err()
                .exit_code()
        );
    }

    #[test]
    fn fail_transaction_more_than_once() {
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        let signers = vec![Address::new_id(ANNE), Address::new_id(BOB)];
        construct_and_verify(&mut rt, signers, NUM_APPROVALS, NO_UNLOCK_DURATION);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(ANNE));
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        let chuck = Address::new_id(CHUCK);
        assert!(propose(
            &mut rt,
            chuck.clone(),
            TokenAmount::from(SEND_VALUE),
            METHOD_SEND,
            fake_params.clone()
        )
        .is_ok());
        rt.verify();
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let proposal_hash_data = make_proposal_hash(
            vec![Address::new_id(ANNE)],
            chuck,
            TokenAmount::from(SEND_VALUE),
            METHOD_SEND,
            fake_params.bytes(),
        );
        assert_eq!(
            ExitCode::ErrIllegalState,
            approve(&mut rt, TXN_ID, proposal_hash_data)
                .unwrap_err()
                .exit_code()
        );
    }

    //#[test]
    fn approve_transaction_that_doesnt_exist() {
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        let signers = vec![Address::new_id(ANNE), Address::new_id(BOB)];
        construct_and_verify(&mut rt, signers, NUM_APPROVALS, NO_UNLOCK_DURATION);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(ANNE));
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        let chuck = Address::new_id(CHUCK);
        assert!(propose(
            &mut rt,
            chuck.clone(),
            TokenAmount::from(SEND_VALUE),
            METHOD_SEND,
            fake_params.clone()
        )
        .is_ok());
        rt.verify();
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(BOB));
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let proposal_hash_data = make_proposal_hash(
            vec![Address::new_id(BOB)],
            chuck,
            TokenAmount::from(SEND_VALUE),
            METHOD_SEND,
            fake_params.bytes(),
        );
        assert_eq!(
            ExitCode::ErrNotFound,
            approve(&mut rt, 1, proposal_hash_data)
                .unwrap_err()
                .exit_code()
        );
        rt.verify();
        assert_transactions(
            &mut rt,
            vec![Transaction {
                to: Address::new_id(CHUCK),
                value: TokenAmount::from(SEND_VALUE),
                method: METHOD_SEND,
                params: fake_params,
                approved: vec![Address::new_id(ANNE)],
            }],
        );
    }

    //#[test]
    fn fail_non_signer() {
        let richard = Address::new_id(105);
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        let signers = vec![Address::new_id(ANNE), Address::new_id(BOB)];
        construct_and_verify(&mut rt, signers, NUM_APPROVALS, NO_UNLOCK_DURATION);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(ANNE));
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        let chuck = Address::new_id(CHUCK);
        assert!(propose(
            &mut rt,
            chuck.clone(),
            TokenAmount::from(SEND_VALUE),
            METHOD_SEND,
            fake_params.clone()
        )
        .is_ok());
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), richard.clone());
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let proposal_hash_data = make_proposal_hash(
            vec![richard.clone()],
            chuck,
            TokenAmount::from(SEND_VALUE),
            METHOD_SEND,
            fake_params.bytes(),
        );
        assert_eq!(
            ExitCode::ErrForbidden,
            approve(&mut rt, TXN_ID, proposal_hash_data)
                .unwrap_err()
                .exit_code()
        );
        rt.verify();
        assert_transactions(
            &mut rt,
            vec![Transaction {
                to: Address::new_id(CHUCK),
                value: TokenAmount::from(SEND_VALUE),
                method: METHOD_SEND,
                params: fake_params,
                approved: vec![Address::new_id(ANNE)],
            }],
        );
    }
}

mod test_cancel {
    use super::*;
    const CHUCK: u64 = 103;
    const RICHARD: u64 = 104;
    const NO_UNLOCK_DURATION: u64 = 0;
    const NUM_APPROVALS: i64 = 2;
    const TXN_ID: i64 = 0;
    const FAKE_METHOD: u64 = 42;
    const SEND_VALUE: u64 = 10;

    fn construct_runtime<'a, BS: BlockStore>(bs: &'a BS) -> MockRuntime<'a, BS> {
        let receiver = Address::new_id(RECEIVER);
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
        let signers = vec![Address::new_id(ANNE), Address::new_id(BOB)];
        construct_and_verify(&mut rt, signers, NUM_APPROVALS, NO_UNLOCK_DURATION);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(ANNE));
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        let chuck = Address::new_id(CHUCK);
        assert!(propose(
            &mut rt,
            chuck.clone(),
            TokenAmount::from(SEND_VALUE),
            METHOD_SEND,
            fake_params.clone()
        )
        .is_ok());
        rt.verify();
        rt.balance = TokenAmount::from(SEND_VALUE);
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let proposal_hash_data = make_proposal_hash(
            vec![Address::new_id(ANNE)],
            chuck,
            TokenAmount::from(SEND_VALUE),
            METHOD_SEND,
            fake_params.bytes(),
        );

        assert!(cancel(&mut rt, TXN_ID, proposal_hash_data).is_ok());
        rt.verify();
        assert_transactions(&mut rt, vec![]);
    }

    #[test]
    fn cancel_with_bad_proposal() {
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        let signers = vec![Address::new_id(ANNE), Address::new_id(BOB)];
        construct_and_verify(&mut rt, signers, NUM_APPROVALS, NO_UNLOCK_DURATION);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(ANNE));
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        let chuck = Address::new_id(CHUCK);
        assert!(propose(
            &mut rt,
            chuck.clone(),
            TokenAmount::from(SEND_VALUE),
            FAKE_METHOD,
            fake_params.clone()
        )
        .is_ok());
        rt.verify();
        rt.balance = TokenAmount::from(SEND_VALUE);
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let proposal_hash_data = make_proposal_hash(
            vec![chuck],
            Address::new_id(BOB),
            TokenAmount::from(SEND_VALUE),
            FAKE_METHOD,
            fake_params.bytes(),
        );

        assert_eq!(
            ExitCode::ErrIllegalState,
            cancel(&mut rt, TXN_ID, proposal_hash_data)
                .unwrap_err()
                .exit_code()
        );
    }

    //#[test]
    fn fail_to_cancel_transaction() {
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        let signers = vec![Address::new_id(ANNE), Address::new_id(BOB)];
        construct_and_verify(&mut rt, signers, NUM_APPROVALS, NO_UNLOCK_DURATION);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(ANNE));
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        let chuck = Address::new_id(CHUCK);
        assert!(propose(
            &mut rt,
            chuck.clone(),
            TokenAmount::from(SEND_VALUE),
            FAKE_METHOD,
            fake_params.clone()
        )
        .is_ok());
        rt.verify();
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(BOB));
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let proposal_hash_data = make_proposal_hash(
            vec![Address::new_id(ANNE)],
            chuck,
            TokenAmount::from(SEND_VALUE),
            FAKE_METHOD,
            fake_params.bytes(),
        );
        assert_eq!(
            ExitCode::ErrForbidden,
            cancel(&mut rt, TXN_ID, proposal_hash_data)
                .unwrap_err()
                .exit_code()
        );
        rt.verify();
        assert_transactions(
            &mut rt,
            vec![Transaction {
                to: Address::new_id(CHUCK),
                value: TokenAmount::from(SEND_VALUE),
                method: FAKE_METHOD,
                params: fake_params,
                approved: vec![Address::new_id(ANNE)],
            }],
        );
    }

    //#[test]
    fn fail_when_not_signer() {
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        let signers = vec![Address::new_id(ANNE), Address::new_id(BOB)];
        construct_and_verify(&mut rt, signers, NUM_APPROVALS, NO_UNLOCK_DURATION);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(ANNE));
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        let chuck = Address::new_id(CHUCK);
        assert!(propose(
            &mut rt,
            chuck.clone(),
            TokenAmount::from(SEND_VALUE),
            FAKE_METHOD,
            fake_params.clone()
        )
        .is_ok());
        rt.verify();
        let richard = Address::new_id(RICHARD);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), richard.clone());
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let proposal_hash_data = make_proposal_hash(
            vec![Address::new_id(ANNE)],
            chuck,
            TokenAmount::from(SEND_VALUE),
            FAKE_METHOD,
            fake_params.bytes(),
        );
        assert_eq!(
            ExitCode::ErrForbidden,
            cancel(&mut rt, TXN_ID, proposal_hash_data)
                .unwrap_err()
                .exit_code()
        );
        rt.verify();
        assert_transactions(
            &mut rt,
            vec![Transaction {
                to: Address::new_id(CHUCK),
                value: TokenAmount::from(SEND_VALUE),
                method: FAKE_METHOD,
                params: fake_params,
                approved: vec![Address::new_id(ANNE)],
            }],
        );
    }

    //#[test]
    fn cancel_transition_doesnt_exist() {
        let bs = MemoryDB::default();
        let mut rt = construct_runtime(&bs);
        let signers = vec![Address::new_id(ANNE), Address::new_id(BOB)];
        construct_and_verify(&mut rt, signers, NUM_APPROVALS, NO_UNLOCK_DURATION);
        rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), Address::new_id(ANNE));
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let fake_params = Serialized::serialize([1, 2, 3, 4]).unwrap();
        let chuck = Address::new_id(CHUCK);
        assert!(propose(
            &mut rt,
            chuck.clone(),
            TokenAmount::from(SEND_VALUE),
            FAKE_METHOD,
            fake_params.clone()
        )
        .is_ok());
        rt.verify();
        rt.expect_validate_caller_type(&[
            ACCOUNT_ACTOR_CODE_ID.clone(),
            MULTISIG_ACTOR_CODE_ID.clone(),
        ]);
        let proposal_hash_data = make_proposal_hash(
            vec![Address::new_id(ANNE)],
            chuck,
            TokenAmount::from(SEND_VALUE),
            FAKE_METHOD,
            fake_params.bytes(),
        );
        assert_eq!(
            ExitCode::ErrNotFound,
            cancel(&mut rt, 1, proposal_hash_data)
                .unwrap_err()
                .exit_code()
        );
        rt.verify();
        assert_transactions(
            &mut rt,
            vec![Transaction {
                to: Address::new_id(CHUCK),
                value: TokenAmount::from(SEND_VALUE),
                method: FAKE_METHOD,
                params: fake_params,
                approved: vec![Address::new_id(ANNE)],
            }],
        );
    }
}

mod test_add_signer {
    use super::*;
    struct SignerTestCase {
        desc: String,
        initial_signers: Vec<Address>,
        initial_approvals: i64,
        add_signer: Address,
        increase: bool,
        expect_signers: Vec<Address>,
        expect_approvals: i64,
        code: ExitCode,
    }
    const CHUCK: u64 = 103;
    const MULTISIG_WALLET_ADD: u64 = 100;
    const NO_LOCK_DURATION: u64 = 0;

    #[test]
    fn test() {
        let test_cases = vec![
            SignerTestCase {
                desc: "happy path add signer".to_string(),
                initial_signers: vec![Address::new_id(ANNE), Address::new_id(BOB)],
                initial_approvals: 2,
                add_signer: Address::new_id(CHUCK),
                increase: false,
                expect_signers: vec![
                    Address::new_id(ANNE),
                    Address::new_id(BOB),
                    Address::new_id(CHUCK),
                ],
                expect_approvals: 2,
                code: ExitCode::Ok,
            },
            SignerTestCase {
                desc: "add signer and increase threshold".to_string(),
                initial_signers: vec![Address::new_id(ANNE), Address::new_id(BOB)],
                initial_approvals: 2,
                add_signer: Address::new_id(CHUCK),
                increase: true,
                expect_signers: vec![
                    Address::new_id(ANNE),
                    Address::new_id(BOB),
                    Address::new_id(CHUCK),
                ],
                expect_approvals: 3,
                code: ExitCode::Ok,
            },
            SignerTestCase {
                desc: "fail to add signer than already exists".to_string(),
                initial_signers: vec![
                    Address::new_id(ANNE),
                    Address::new_id(BOB),
                    Address::new_id(CHUCK),
                ],
                initial_approvals: 3,
                add_signer: Address::new_id(CHUCK),
                increase: false,
                expect_signers: vec![
                    Address::new_id(ANNE),
                    Address::new_id(BOB),
                    Address::new_id(CHUCK),
                ],
                expect_approvals: 3,
                code: ExitCode::ErrIllegalArgument,
            },
        ];

        for test_case in test_cases {
            println!("Test case executing is {}", test_case.desc);
            let receiver = Address::new_id(MULTISIG_WALLET_ADD);
            let message = UnsignedMessage::builder()
                .to(receiver.clone())
                .from(SYSTEM_ACTOR_ADDR.clone())
                .build()
                .unwrap();
            let bs = MemoryDB::default();
            let mut rt = MockRuntime::new(&bs, message);
            rt.set_caller(INIT_ACTOR_CODE_ID.clone(), INIT_ACTOR_ADDR.clone());

            construct_and_verify(
                &mut rt,
                test_case.initial_signers,
                test_case.initial_approvals,
                NO_LOCK_DURATION,
            );
            rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), receiver.clone());
            rt.expect_validate_caller_addr(&[receiver.clone()]);
            if test_case.code == ExitCode::Ok {
                assert!(add_signer(&mut rt, test_case.add_signer, test_case.increase).is_ok());
                let state: State = rt.get_state().unwrap();
                assert_eq!(test_case.expect_signers, state.signers);
                assert_eq!(test_case.expect_approvals, state.num_approvals_threshold);
            } else {
                assert_eq!(
                    test_case.code,
                    add_signer(&mut rt, test_case.add_signer, test_case.increase)
                        .unwrap_err()
                        .exit_code()
                );
            }
            rt.verify();
        }
    }
}

mod test_remove_signer {
    use super::*;
    struct SignerTestCase {
        desc: String,
        initial_signers: Vec<Address>,
        initial_approvals: i64,
        remove_signer: Address,
        decrease: bool,
        expect_signers: Vec<Address>,
        expect_approvals: i64,
        code: ExitCode,
    }
    const CHUCK: u64 = 103;
    const RICHARD: u64 = 104;
    const MULTISIG_WALLET_ADD: u64 = 100;
    const NO_LOCK_DURATION: u64 = 0;

    //#[test]
    fn test() {
        let test_cases = vec![
            SignerTestCase {
                desc: "happy path add signer".to_string(),
                initial_signers: vec![
                    Address::new_id(ANNE),
                    Address::new_id(BOB),
                    Address::new_id(CHUCK),
                ],
                initial_approvals: 2,
                remove_signer: Address::new_id(CHUCK),
                decrease: false,
                expect_signers: vec![Address::new_id(ANNE), Address::new_id(BOB)],
                expect_approvals: 2,
                code: ExitCode::Ok,
            },
            SignerTestCase {
                desc: "Remove signer and decrease threshold".to_string(),
                initial_signers: vec![
                    Address::new_id(ANNE),
                    Address::new_id(BOB),
                    Address::new_id(CHUCK),
                ],
                initial_approvals: 2,
                remove_signer: Address::new_id(CHUCK),
                decrease: true,
                expect_signers: vec![Address::new_id(ANNE), Address::new_id(BOB)],
                expect_approvals: 1,
                code: ExitCode::Ok,
            },
            SignerTestCase {
                desc: "Remove signer with automatic threhold decrease".to_string(),
                initial_signers: vec![
                    Address::new_id(ANNE),
                    Address::new_id(BOB),
                    Address::new_id(CHUCK),
                ],
                initial_approvals: 3,
                remove_signer: Address::new_id(CHUCK),
                decrease: false,
                expect_signers: vec![Address::new_id(ANNE), Address::new_id(BOB)],
                expect_approvals: 2,
                code: ExitCode::Ok,
            },
            SignerTestCase {
                desc: "Remove signer from single signer list".to_string(),
                initial_signers: vec![Address::new_id(ANNE)],
                initial_approvals: 2,
                remove_signer: Address::new_id(ANNE),
                decrease: false,
                expect_signers: vec![],
                expect_approvals: 2,
                code: ExitCode::ErrForbidden,
            },
            SignerTestCase {
                desc: "Fail to remove non signer".to_string(),
                initial_signers: vec![
                    Address::new_id(ANNE),
                    Address::new_id(BOB),
                    Address::new_id(CHUCK),
                ],
                initial_approvals: 2,
                remove_signer: Address::new_id(RICHARD),
                decrease: false,
                expect_signers: vec![
                    Address::new_id(ANNE),
                    Address::new_id(BOB),
                    Address::new_id(CHUCK),
                ],
                expect_approvals: 2,
                code: ExitCode::ErrNotFound,
            },
        ];
        for test_case in test_cases {
            println!("Test case executing is {}", test_case.desc);
            let receiver = Address::new_id(MULTISIG_WALLET_ADD);
            let message = UnsignedMessage::builder()
                .to(receiver.clone())
                .from(SYSTEM_ACTOR_ADDR.clone())
                .build()
                .unwrap();
            let bs = MemoryDB::default();
            let mut rt = MockRuntime::new(&bs, message);
            rt.set_caller(INIT_ACTOR_CODE_ID.clone(), INIT_ACTOR_ADDR.clone());

            construct_and_verify(
                &mut rt,
                test_case.initial_signers,
                test_case.initial_approvals,
                NO_LOCK_DURATION,
            );
            rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), receiver.clone());
            rt.expect_validate_caller_addr(&[receiver.clone()]);
            if test_case.code == ExitCode::Ok {
                assert!(
                    remove_signer(&mut rt, test_case.remove_signer, test_case.decrease).is_ok()
                );
                let state: State = rt.get_state().unwrap();
                assert_eq!(test_case.expect_signers, state.signers);
                assert_eq!(test_case.expect_approvals, state.num_approvals_threshold);
            } else {
                assert_eq!(
                    test_case.code,
                    add_signer(&mut rt, test_case.remove_signer, test_case.decrease)
                        .unwrap_err()
                        .exit_code()
                );
            }
            rt.verify();
        }
    }
}

mod test_swap_signers {
    use super::*;
    struct SwapTestCase {
        desc: String,
        to: Address,
        from: Address,
        expect: Vec<Address>,
        code: ExitCode,
    }
    const CHUCK: u64 = 103;
    const DARLENE: u64 = 104;
    const MULTISIG_WALLET_ADD: u64 = 100;
    const NO_LOCK_DURATION: u64 = 0;
    const NUM_APPROVALS: i64 = 1;

    #[test]
    fn test() {
        let test_cases = vec![
            SwapTestCase {
                desc: "happy path signer swap".to_string(),
                to: Address::new_id(CHUCK),
                from: Address::new_id(BOB),
                expect: vec![Address::new_id(ANNE), Address::new_id(CHUCK)],
                code: ExitCode::Ok,
            },
            SwapTestCase {
                desc: "fail to swap when from signer not found".to_string(),
                to: Address::new_id(CHUCK),
                from: Address::new_id(DARLENE),
                expect: vec![Address::new_id(ANNE), Address::new_id(CHUCK)],
                code: ExitCode::ErrNotFound,
            },
            SwapTestCase {
                desc: "fail to swap when to signer already present".to_string(),
                to: Address::new_id(BOB),
                from: Address::new_id(ANNE),
                expect: vec![Address::new_id(ANNE), Address::new_id(CHUCK)],
                code: ExitCode::ErrIllegalArgument,
            },
        ];
        let initial_signer = vec![Address::new_id(ANNE), Address::new_id(BOB)];
        for test_case in test_cases {
            println!("Test case executing is {}", test_case.desc);
            let receiver = Address::new_id(MULTISIG_WALLET_ADD);
            let message = UnsignedMessage::builder()
                .to(receiver.clone())
                .from(SYSTEM_ACTOR_ADDR.clone())
                .build()
                .unwrap();
            let bs = MemoryDB::default();
            let mut rt = MockRuntime::new(&bs, message);
            rt.set_caller(INIT_ACTOR_CODE_ID.clone(), INIT_ACTOR_ADDR.clone());

            construct_and_verify(
                &mut rt,
                initial_signer.clone(),
                NUM_APPROVALS,
                NO_LOCK_DURATION,
            );
            rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), receiver.clone());

            rt.expect_validate_caller_addr(&[receiver.clone()]);
            if test_case.code == ExitCode::Ok {
                assert!(swap_signers(&mut rt, test_case.from, test_case.to).is_ok());
                let state: State = rt.get_state().unwrap();
                assert_eq!(test_case.expect, state.signers);
            } else {
                assert_eq!(
                    test_case.code,
                    swap_signers(&mut rt, test_case.from, test_case.to)
                        .unwrap_err()
                        .exit_code()
                );
            }
            rt.verify();
        }
    }
}

mod test_change_treshold {
    use super::*;
    const CHUCK: u64 = 103;
    const MULTISIG_WALLET_ADD: u64 = 100;
    const NO_LOCK_DURATION: u64 = 0;
    struct Threshold {
        desc: String,
        initial_threshold: i64,
        setThreshold: i64,
        code: ExitCode,
    }

    #[test]
    fn test() {
        let initial_signer = vec![
            Address::new_id(ANNE),
            Address::new_id(BOB),
            Address::new_id(CHUCK),
        ];
        let test_cases = vec![
            Threshold {
                desc: "happy path decrease threshold".to_string(),
                initial_threshold: 2,
                setThreshold: 1,
                code: ExitCode::Ok,
            },
            Threshold {
                desc: "happy path simple increase threshold".to_string(),
                initial_threshold: 2,
                setThreshold: 3,
                code: ExitCode::Ok,
            },
            Threshold {
                desc: "fail to set threshold to zero".to_string(),
                initial_threshold: 2,
                setThreshold: 0,
                code: ExitCode::ErrIllegalArgument,
            },
            Threshold {
                desc: "fail to set threshold less than zero".to_string(),
                initial_threshold: 2,
                setThreshold: -1,
                code: ExitCode::ErrIllegalArgument,
            },
            Threshold {
                desc: "fail to set threshold above number of signers".to_string(),
                initial_threshold: 2,
                setThreshold: initial_signer.len() as i64 + 1,
                code: ExitCode::ErrIllegalArgument,
            },
        ];
        for test_case in test_cases {
            println!("Test case executing is {}", test_case.desc);
            let receiver = Address::new_id(MULTISIG_WALLET_ADD);
            let message = UnsignedMessage::builder()
                .to(receiver.clone())
                .from(SYSTEM_ACTOR_ADDR.clone())
                .build()
                .unwrap();
            let bs = MemoryDB::default();
            let mut rt = MockRuntime::new(&bs, message);
            rt.set_caller(INIT_ACTOR_CODE_ID.clone(), INIT_ACTOR_ADDR.clone());
            construct_and_verify(
                &mut rt,
                initial_signer.clone(),
                test_case.initial_threshold,
                NO_LOCK_DURATION,
            );
            rt.set_caller(ACCOUNT_ACTOR_CODE_ID.clone(), receiver.clone());
            rt.expect_validate_caller_addr(&[receiver.clone()]);
            if test_case.code == ExitCode::Ok {
                assert!(change_num_approvals_threshold(&mut rt, test_case.setThreshold).is_ok());
                let state: State = rt.get_state().unwrap();
                assert_eq!(test_case.setThreshold, state.num_approvals_threshold);
            } else {
                assert_eq!(
                    test_case.code,
                    change_num_approvals_threshold(&mut rt, test_case.setThreshold)
                        .unwrap_err()
                        .exit_code()
                );
            }
            rt.verify();
        }
    }
}
