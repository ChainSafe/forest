// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::{
    self, ACCOUNT_ACTOR_CODE_ID, CRON_ACTOR_CODE_ID, INIT_ACTOR_CODE_ID, MARKET_ACTOR_CODE_ID,
    MINER_ACTOR_CODE_ID, MULTISIG_ACTOR_CODE_ID, PAYCH_ACTOR_CODE_ID, POWER_ACTOR_CODE_ID,
    REWARD_ACTOR_CODE_ID, SYSTEM_ACTOR_CODE_ID, VERIFIED_ACTOR_CODE_ID,
};
use address::Address;
use cid::{multihash::Blake2b256, Cid};
use clock::ChainEpoch;
use crypto::{DomainSeparationTag, Signature};
use db::MemoryDB;
use encoding::{blake2b_256, de::DeserializeOwned, Cbor};
use fil_types::{PieceInfo, RegisteredSealProof, SealVerifyInfo, WindowPoStVerifyInfo};
use ipld_blockstore::BlockStore;
use runtime::{ActorCode, ConsensusFault, MessageInfo, Runtime, Syscalls};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::error::Error as StdError;
use vm::{ActorError, ExitCode, MethodNum, Randomness, Serialized, TokenAmount};

pub struct MockRuntime {
    pub epoch: ChainEpoch,
    pub miner: Address,
    pub id_addresses: HashMap<Address, Address>,
    pub actor_code_cids: HashMap<Address, Cid>,
    pub new_actor_addr: Option<Address>,
    pub receiver: Address,
    pub caller: Address,
    pub caller_type: Cid,
    pub value_received: TokenAmount,

    // TODO: syscalls: syscaller

    // Actor State
    pub state: Option<Cid>,
    pub balance: TokenAmount,
    pub received: TokenAmount,

    // VM Impl
    pub in_call: bool,
    pub store: MemoryDB,
    pub in_transaction: bool,

    // Expectations
    pub expect_validate_caller_any: Cell<bool>,
    pub expect_validate_caller_addr: RefCell<Option<Vec<Address>>>,
    pub expect_validate_caller_type: RefCell<Option<Vec<Cid>>>,
    pub expect_sends: VecDeque<ExpectedMessage>,
    pub expect_create_actor: Option<ExpectCreateActor>,
    pub expect_verify_sigs: RefCell<Vec<ExpectedVerifySig>>,
    pub expect_verify_seal: RefCell<Option<ExpectVerifySeal>>,
    pub expect_verify_post: RefCell<Option<ExpectVerifyPoSt>>,
    pub expect_compute_unsealed_sector_cid: RefCell<Option<ExpectComputeUnsealedSectorCid>>,
    pub expect_verify_consensus_fault: RefCell<Option<ExpectVerifyConsensusFault>>,
}

impl Default for MockRuntime {
    fn default() -> Self {
        Self {
            epoch: Default::default(),
            miner: Address::new_id(0),
            id_addresses: Default::default(),
            actor_code_cids: Default::default(),
            new_actor_addr: Default::default(),
            receiver: Address::new_id(0),
            caller: Address::new_id(0),
            caller_type: Default::default(),
            value_received: Default::default(),
            state: Default::default(),
            balance: Default::default(),
            received: Default::default(),
            in_call: Default::default(),
            store: Default::default(),
            in_transaction: Default::default(),
            expect_validate_caller_any: Default::default(),
            expect_validate_caller_addr: Default::default(),
            expect_validate_caller_type: Default::default(),
            expect_sends: Default::default(),
            expect_create_actor: Default::default(),
            expect_verify_sigs: Default::default(),
            expect_verify_seal: Default::default(),
            expect_verify_post: Default::default(),
            expect_compute_unsealed_sector_cid: Default::default(),
            expect_verify_consensus_fault: Default::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExpectCreateActor {
    pub code_id: Cid,
    pub address: Address,
}
#[derive(Clone, Debug)]
pub struct ExpectedMessage {
    pub to: Address,
    pub method: MethodNum,
    pub params: Serialized,
    pub value: TokenAmount,

    // returns from applying expectedMessage
    pub send_return: Serialized,
    pub exit_code: ExitCode,
}

#[derive(Clone, Debug)]
pub struct ExpectedVerifySig {
    pub sig: Signature,
    pub signer: Address,
    pub plaintext: Vec<u8>,
    pub result: ExitCode,
}

#[derive(Clone, Debug)]
pub struct ExpectVerifySeal {
    seal: SealVerifyInfo,
    exit_code: ExitCode,
}

#[derive(Clone, Debug)]
pub struct ExpectVerifyPoSt {
    post: WindowPoStVerifyInfo,
    exit_code: ExitCode,
}

#[derive(Clone)]
pub struct ExpectVerifyConsensusFault {
    require_correct_input: bool,
    block_header_1: Vec<u8>,
    block_header_2: Vec<u8>,
    block_header_extra: Vec<u8>,
    fault: Option<ConsensusFault>,
    exit_code: ExitCode,
}

#[derive(Clone)]
pub struct ExpectComputeUnsealedSectorCid {
    reg: RegisteredSealProof,
    pieces: Vec<PieceInfo>,
    cid: Cid,
    exit_code: ExitCode,
}

impl MockRuntime {
    fn require_in_call(&self) {
        assert!(
            self.in_call,
            "invalid runtime invocation outside of method call",
        )
    }
    fn check_argument(&self, predicate: bool, msg: String) -> Result<(), ActorError> {
        if !predicate {
            return Err(ActorError::new(ExitCode::SysErrorIllegalArgument, msg));
        }
        Ok(())
    }
    fn put<C: Cbor>(&self, o: &C) -> Result<Cid, ActorError> {
        Ok(self.store.put(&o, Blake2b256).unwrap())
    }
    fn _get<T: DeserializeOwned>(&self, cid: Cid) -> Result<T, ActorError> {
        Ok(self.store.get(&cid).unwrap().unwrap())
    }

    #[allow(dead_code)]
    pub fn get_state<T: DeserializeOwned>(&self) -> Result<T, ActorError> {
        let data: T = self
            .store
            .get(&self.state.as_ref().unwrap())
            .unwrap()
            .unwrap();
        Ok(data)
    }
    pub fn expect_validate_caller_addr(&self, addr: &[Address]) {
        assert!(addr.len() > 0, "addrs must be non-empty");
        *self.expect_validate_caller_addr.borrow_mut() = Some(addr.to_vec());
    }

    #[allow(dead_code)]
    pub fn expect_verify_signature(&self, exp: ExpectedVerifySig) {
        self.expect_verify_sigs.borrow_mut().push(exp);
    }

    #[allow(dead_code)]
    pub fn expect_verify_consensus_fault(
        &self,
        h1: Vec<u8>,
        h2: Vec<u8>,
        extra: Vec<u8>,
        fault: Option<ConsensusFault>,
        exit_code: ExitCode,
    ) {
        *self.expect_verify_consensus_fault.borrow_mut() = Some(ExpectVerifyConsensusFault {
            require_correct_input: true,
            block_header_1: h1,
            block_header_2: h2,
            block_header_extra: extra,
            fault: fault,
            exit_code: exit_code,
        });
    }

    #[allow(dead_code)]
    pub fn expect_compute_unsealed_sector_cid(&self, exp: ExpectComputeUnsealedSectorCid) {
        *self.expect_compute_unsealed_sector_cid.borrow_mut() = Some(exp);
    }

    #[allow(dead_code)]
    pub fn expect_validate_caller_type(&self, types: &[Cid]) {
        assert!(types.len() > 0, "addrs must be non-empty");
        *self.expect_validate_caller_type.borrow_mut() = Some(types.to_vec());
    }

    #[allow(dead_code)]
    pub fn expect_validate_caller_any(&self) {
        self.expect_validate_caller_any.set(true);
    }

    pub fn call(
        &mut self,
        to_code: &Cid,
        method_num: MethodNum,
        params: &Serialized,
    ) -> Result<Serialized, ActorError> {
        self.in_call = true;
        let prev_state = self.state.clone();

        let res = match to_code {
            x if x == &*SYSTEM_ACTOR_CODE_ID => {
                actor::system::Actor.invoke_method(self, method_num, params)
            }
            x if x == &*INIT_ACTOR_CODE_ID => {
                actor::init::Actor.invoke_method(self, method_num, params)
            }
            x if x == &*CRON_ACTOR_CODE_ID => {
                actor::cron::Actor.invoke_method(self, method_num, params)
            }
            x if x == &*ACCOUNT_ACTOR_CODE_ID => {
                actor::account::Actor.invoke_method(self, method_num, params)
            }
            x if x == &*POWER_ACTOR_CODE_ID => {
                actor::power::Actor.invoke_method(self, method_num, params)
            }
            x if x == &*MINER_ACTOR_CODE_ID => {
                actor::miner::Actor.invoke_method(self, method_num, params)
            }
            x if x == &*MARKET_ACTOR_CODE_ID => {
                actor::market::Actor.invoke_method(self, method_num, params)
            }
            x if x == &*PAYCH_ACTOR_CODE_ID => {
                actor::paych::Actor.invoke_method(self, method_num, params)
            }
            x if x == &*MULTISIG_ACTOR_CODE_ID => {
                actor::multisig::Actor.invoke_method(self, method_num, params)
            }
            x if x == &*REWARD_ACTOR_CODE_ID => {
                actor::reward::Actor.invoke_method(self, method_num, params)
            }
            x if x == &*VERIFIED_ACTOR_CODE_ID => {
                actor::verifreg::Actor.invoke_method(self, method_num, params)
            }
            _ => Err(ActorError::new(
                ExitCode::SysErrForbidden,
                "invalid method id".to_owned(),
            )),
        };

        if res.is_err() {
            self.state = prev_state;
        }
        self.in_call = false;
        return res;
    }
    pub fn verify(&mut self) {
        assert!(
            !self.expect_validate_caller_any.get(),
            "expected ValidateCallerAny, not received"
        );
        assert!(
            self.expect_validate_caller_addr.borrow().as_ref().is_none(),
            "expected ValidateCallerAddr {:?}, not received",
            self.expect_validate_caller_addr.borrow().as_ref().unwrap()
        );
        assert!(
            self.expect_validate_caller_type.borrow().as_ref().is_none(),
            "expected ValidateCallerType {:?}, not received",
            self.expect_validate_caller_type.borrow().as_ref().unwrap()
        );
        assert!(
            self.expect_sends.is_empty(),
            "expected all message to be send, unsent messages {:?}",
            self.expect_sends
        );
        assert!(
            self.expect_create_actor.is_none(),
            "expected actor to be created, uncreated actor: {:?}",
            self.expect_create_actor
        );
        assert!(
            self.expect_verify_seal.borrow().as_ref().is_none(),
            "expect_verify_seal {:?}, not received",
            self.expect_verify_seal.borrow().as_ref().unwrap()
        );
        assert!(
            self.expect_compute_unsealed_sector_cid
                .borrow()
                .as_ref()
                .is_none(),
            "expect_compute_unsealed_sector_cid not received",
        );
        assert!(
            self.expect_verify_consensus_fault
                .borrow()
                .as_ref()
                .is_none(),
            "expect_compute_unsealed_sector_cid not received",
        );

        self.reset();
    }
    pub fn reset(&mut self) {
        self.expect_validate_caller_any.set(false);
        *self.expect_validate_caller_addr.borrow_mut() = None;
        *self.expect_validate_caller_type.borrow_mut() = None;
        self.expect_create_actor = None;
        self.expect_verify_sigs.borrow_mut().clear();
        *self.expect_verify_seal.borrow_mut() = None;
        *self.expect_verify_post.borrow_mut() = None;
        *self.expect_compute_unsealed_sector_cid.borrow_mut() = None;
        *self.expect_verify_consensus_fault.borrow_mut() = None;
    }

    #[allow(dead_code)]
    pub fn expect_send(
        &mut self,
        to: Address,
        method: MethodNum,
        params: Serialized,
        value: TokenAmount,
        send_return: Serialized,
        exit_code: ExitCode,
    ) {
        self.expect_sends.push_back(ExpectedMessage {
            to,
            method,
            params,
            value,
            send_return,
            exit_code,
        })
    }

    #[allow(dead_code)]
    pub fn expect_create_actor(&mut self, code_id: Cid, address: Address) {
        let a = ExpectCreateActor { code_id, address };
        self.expect_create_actor = Some(a);
    }

    #[allow(dead_code)]
    pub fn expect_verify_seal(&mut self, seal: SealVerifyInfo, exit_code: ExitCode) {
        let a = ExpectVerifySeal { seal, exit_code };
        *self.expect_verify_seal.borrow_mut() = Some(a);
    }

    #[allow(dead_code)]
    pub fn expect_verify_post(&mut self, post: WindowPoStVerifyInfo, exit_code: ExitCode) {
        let a = ExpectVerifyPoSt { post, exit_code };
        *self.expect_verify_post.borrow_mut() = Some(a);
    }

    #[allow(dead_code)]
    pub fn set_caller(&mut self, code_id: Cid, address: Address) {
        self.caller = address;
        self.caller_type = code_id.clone();
        self.actor_code_cids.insert(address, code_id);
    }

    #[allow(dead_code)]
    pub fn set_value(&mut self, value: TokenAmount) {
        self.value_received = value;
    }
}

impl MessageInfo for MockRuntime {
    fn caller(&self) -> &Address {
        &self.caller
    }
    fn receiver(&self) -> &Address {
        &self.receiver
    }
    fn value_received(&self) -> &TokenAmount {
        &self.value_received
    }
}

impl Runtime<MemoryDB> for MockRuntime {
    fn message(&self) -> &dyn MessageInfo {
        self.require_in_call();
        self
    }

    fn curr_epoch(&self) -> ChainEpoch {
        self.require_in_call();
        self.epoch
    }

    fn validate_immediate_caller_accept_any(&self) {
        self.require_in_call();
        assert!(
            self.expect_validate_caller_any.get(),
            "unexpected validate-caller-any"
        );
        self.expect_validate_caller_any.set(false);
    }

    fn validate_immediate_caller_is<'a, I>(&self, addresses: I) -> Result<(), ActorError>
    where
        I: IntoIterator<Item = &'a Address>,
    {
        self.require_in_call();

        let addrs: Vec<Address> = addresses.into_iter().cloned().collect();

        self.check_argument(addrs.len() > 0, "addrs must be non-empty".to_owned())?;

        assert!(
            self.expect_validate_caller_addr.borrow().is_some(),
            "unexpected validate caller addrs"
        );
        assert!(
            &addrs == self.expect_validate_caller_addr.borrow().as_ref().unwrap(),
            "unexpected validate caller addrs {:?}, expected {:?}",
            addrs,
            self.expect_validate_caller_addr.borrow().as_ref()
        );

        for expected in &addrs {
            if self.message().caller() == expected {
                *self.expect_validate_caller_addr.borrow_mut() = None;
                return Ok(());
            }
        }
        *self.expect_validate_caller_addr.borrow_mut() = None;
        return Err(ActorError::new(
            ExitCode::ErrForbidden,
            format!(
                "caller address {:?} forbidden, allowed: {:?}",
                self.message().caller(),
                &addrs
            ),
        ));
    }
    fn validate_immediate_caller_type<'a, I>(&self, types: I) -> Result<(), ActorError>
    where
        I: IntoIterator<Item = &'a Cid>,
    {
        self.require_in_call();
        let types: Vec<Cid> = types.into_iter().cloned().collect();

        self.check_argument(types.len() > 0, "types must be non-empty".to_owned())?;

        assert!(
            self.expect_validate_caller_type.borrow().is_some(),
            "unexpected validate caller code"
        );
        assert!(
            &types == self.expect_validate_caller_type.borrow().as_ref().unwrap(),
            "unexpected validate caller code {:?}, expected {:?}",
            types,
            self.expect_validate_caller_type
        );

        for expected in &types {
            if &self.caller_type == expected {
                *self.expect_validate_caller_type.borrow_mut() = None;
                return Ok(());
            }
        }

        *self.expect_validate_caller_type.borrow_mut() = None;

        Err(self.abort(
            ExitCode::ErrForbidden,
            format!(
                "caller type {:?} forbidden, allowed: {:?}",
                self.caller_type, types
            ),
        ))
    }

    fn current_balance(&self) -> Result<TokenAmount, ActorError> {
        self.require_in_call();
        Ok(self.balance.clone())
    }

    fn resolve_address(&self, address: &Address) -> Result<Address, ActorError> {
        self.require_in_call();
        if address.protocol() == address::Protocol::ID {
            return Ok(address.clone());
        }

        self.id_addresses
            .get(&address)
            .cloned()
            .ok_or(ActorError::new(
                ExitCode::ErrIllegalArgument,
                "Address not found".to_string(),
            ))
    }

    fn get_actor_code_cid(&self, addr: &Address) -> Result<Cid, ActorError> {
        self.require_in_call();

        self.actor_code_cids
            .get(&addr)
            .cloned()
            .ok_or(ActorError::new(
                ExitCode::ErrIllegalArgument,
                "Actor address is not found".to_string(),
            ))
    }

    fn get_randomness(
        &self,
        _personalization: DomainSeparationTag,
        _rand_epoch: ChainEpoch,
        _entropy: &[u8],
    ) -> Result<Randomness, ActorError> {
        unimplemented!()
    }

    fn create<C: Cbor>(&mut self, obj: &C) -> Result<(), ActorError> {
        if self.state.is_some() == true {
            return Err(self.abort(
                ExitCode::SysErrorIllegalActor,
                "state already constructed".to_owned(),
            ));
        }
        self.state = Some(self.store.put(obj, Blake2b256).unwrap());
        Ok(())
    }

    fn state<C: Cbor>(&self) -> Result<C, ActorError> {
        Ok(self
            .store
            .get(&self.state.as_ref().unwrap())
            .unwrap()
            .unwrap())
    }

    fn transaction<C: Cbor, R, F>(&mut self, f: F) -> Result<R, ActorError>
    where
        F: FnOnce(&mut C, &mut Self) -> R,
    {
        if self.in_transaction {
            return Err(self.abort(ExitCode::SysErrorIllegalActor, "nested transaction"));
        }
        let mut read_only = self.state()?;
        self.in_transaction = true;
        let ret = f(&mut read_only, self);
        self.state = Some(self.put(&read_only).unwrap());
        self.in_transaction = false;
        Ok(ret)
    }

    fn store(&self) -> &MemoryDB {
        &self.store
    }

    fn send(
        &mut self,
        to: &Address,
        method: MethodNum,
        params: &Serialized,
        value: &TokenAmount,
    ) -> Result<Serialized, ActorError> {
        self.require_in_call();
        if self.in_transaction {
            return Err(self.abort(
                ExitCode::SysErrorIllegalActor,
                "side-effect within transaction",
            ));
        }

        assert!(
            !self.expect_sends.is_empty(),
            "unexpected expectedMessage to: {:?} method: {:?}, value: {:?}, params: {:?}",
            to,
            method,
            value,
            params
        );

        let expected_msg = self.expect_sends.pop_front().unwrap();

        assert!(&expected_msg.to == to && expected_msg.method == method && &expected_msg.params == params && &expected_msg.value == value, "expectedMessage being sent does not match expectation.\nMessage -\t to: {:?} method: {:?} value: {:?} params: {:?}\nExpected -\t {:?}", to, method, value, params, self.expect_sends[0]);

        if value > &self.balance {
            return Err(self.abort(
                ExitCode::SysErrSenderStateInvalid,
                format!(
                    "cannot send value: {:?} exceeds balance: {:?}",
                    value, self.balance
                ),
            ));
        }
        self.balance -= value;

        match expected_msg.exit_code {
            ExitCode::Ok => return Ok(expected_msg.send_return),
            x => {
                return Err(ActorError::new(x, "Expected message Fail".to_string()));
            }
        }
    }

    fn abort<S: AsRef<str>>(&self, exit_code: ExitCode, msg: S) -> ActorError {
        ActorError::new(exit_code, msg.as_ref().to_owned())
    }

    fn new_actor_address(&mut self) -> Result<Address, ActorError> {
        self.require_in_call();
        let ret = self
            .new_actor_addr
            .as_ref()
            .expect("unexpected call to new actor address")
            .clone();
        self.new_actor_addr = None;
        return Ok(ret);
    }

    fn create_actor(&mut self, code_id: &Cid, address: &Address) -> Result<(), ActorError> {
        self.require_in_call();
        if self.in_transaction {
            return Err(self.abort(
                ExitCode::SysErrorIllegalActor,
                "side-effect within transaction".to_owned(),
            ));
        }
        let expect_create_actor = self
            .expect_create_actor
            .take()
            .expect("unexpected call to create actor");

        assert!(&expect_create_actor.code_id == code_id && &expect_create_actor.address == address, "unexpected actor being created, expected code: {:?} address: {:?}, actual code: {:?} address: {:?}", expect_create_actor.code_id, expect_create_actor.address, code_id, address);
        Ok(())
    }

    fn delete_actor(&mut self, _beneficiary: &Address) -> Result<(), ActorError> {
        self.require_in_call();
        if self.in_transaction {
            return Err(self.abort(
                ExitCode::SysErrorIllegalActor,
                "side-effect within transaction".to_owned(),
            ));
        }
        todo!("implement me???")
    }

    fn total_fil_circ_supply(&self) -> Result<TokenAmount, ActorError> {
        unimplemented!();
    }

    fn syscalls(&self) -> &dyn Syscalls {
        self
    }
}

impl Syscalls for MockRuntime {
    fn verify_signature(
        &self,
        signature: &Signature,
        signer: &Address,
        plaintext: &[u8],
    ) -> Result<(), Box<dyn StdError>> {
        if self.expect_verify_sigs.borrow().len() == 0 {
            return Err(Box::new(ActorError::new(
                ExitCode::ErrIllegalState,
                "Unexpected signature verification".to_string(),
            )));
        }
        let exp = self
            .expect_verify_sigs
            .borrow_mut()
            .pop()
            .ok_or(ActorError::new(
                ExitCode::ErrIllegalState,
                "Unexpected signature verification".to_string(),
            ))?;
        if exp.sig == *signature && exp.signer == *signer && &exp.plaintext[..] == plaintext {
            if exp.result == ExitCode::Ok {
                return Ok(());
            } else {
                return Err(Box::new(ActorError::new(
                    exp.result,
                    "Expected failure".to_string(),
                )));
            }
        } else {
            return Err(Box::new(ActorError::new(
                ExitCode::ErrIllegalState,
                "Signatures did not match".to_string(),
            )));
        }
    }

    fn hash_blake2b(&self, data: &[u8]) -> Result<[u8; 32], Box<dyn StdError>> {
        Ok(blake2b_256(&data))
    }
    fn compute_unsealed_sector_cid(
        &self,
        reg: RegisteredSealProof,
        pieces: &[PieceInfo],
    ) -> Result<Cid, Box<dyn StdError>> {
        let exp = self
            .expect_compute_unsealed_sector_cid
            .replace(None)
            .ok_or(Box::new(ActorError::new(
                ExitCode::ErrIllegalState,
                "Unexpected syscall to ComputeUnsealedSectorCID".to_string(),
            )))?;

        if exp.reg != reg {
            return Err(Box::new(ActorError::new(
                ExitCode::ErrIllegalState,
                "Unexpected compute_unsealed_sector_cid : reg mismatch".to_string(),
            )));
        }

        if exp.pieces[..].eq(pieces) {
            return Err(Box::new(ActorError::new(
                ExitCode::ErrIllegalState,
                "Unexpected compute_unsealed_sector_cid : pieces mismatch".to_string(),
            )));
        }

        if exp.exit_code != ExitCode::Ok {
            return Err(Box::new(ActorError::new(
                exp.exit_code,
                "Expected Failure".to_string(),
            )));
        }
        Ok(exp.cid)
    }
    fn verify_seal(&self, seal: &SealVerifyInfo) -> Result<(), Box<dyn StdError>> {
        let exp = self
            .expect_verify_seal
            .replace(None)
            .ok_or(Box::new(ActorError::new(
                ExitCode::ErrIllegalState,
                "Unexpected syscall to verify seal".to_string(),
            )))?;

        if exp.seal != *seal {
            return Err(Box::new(ActorError::new(
                ExitCode::ErrIllegalState,
                "Unexpected seal verification".to_string(),
            )));
        }
        if exp.exit_code != ExitCode::Ok {
            return Err(Box::new(ActorError::new(
                exp.exit_code,
                "Expected Failure".to_string(),
            )));
        }
        Ok(())
    }
    fn verify_post(&self, post: &WindowPoStVerifyInfo) -> Result<(), Box<dyn StdError>> {
        let exp = self
            .expect_verify_post
            .replace(None)
            .ok_or(Box::new(ActorError::new(
                ExitCode::ErrIllegalState,
                "Unexpected syscall to verify PoSt ".to_string(),
            )))?;

        if exp.post != *post {
            return Err(Box::new(ActorError::new(
                ExitCode::ErrIllegalState,
                "Unexpected PoSt verification".to_string(),
            )));
        }
        if exp.exit_code != ExitCode::Ok {
            return Err(Box::new(ActorError::new(
                exp.exit_code,
                "Expected Failure".to_string(),
            )));
        }
        Ok(())
    }
    fn verify_consensus_fault(
        &self,
        h1: &[u8],
        h2: &[u8],
        extra: &[u8],
    ) -> Result<Option<ConsensusFault>, Box<dyn StdError>> {
        let exp = self
            .expect_verify_consensus_fault
            .replace(None)
            .ok_or(Box::new(ActorError::new(
                ExitCode::ErrIllegalState,
                "Unexpected syscall to verify_consensus_fault".to_string(),
            )))?;
        if exp.require_correct_input {
            if exp.block_header_1 != h1 {
                return Err(Box::new(ActorError::new(
                    ExitCode::ErrIllegalState,
                    "Header 1 mismatch".to_string(),
                )));
            }
            if exp.block_header_2 != h2 {
                return Err(Box::new(ActorError::new(
                    ExitCode::ErrIllegalState,
                    "Header 2 mismatch".to_string(),
                )));
            }
            if exp.block_header_extra != extra {
                return Err(Box::new(ActorError::new(
                    ExitCode::ErrIllegalState,
                    "Header extra mismatch".to_string(),
                )));
            }
        }
        if exp.exit_code != ExitCode::Ok {
            return Err(Box::new(ActorError::new(
                exp.exit_code,
                "Expected Failure".to_string(),
            )));
        }
        Ok(exp.fault)
    }
}
