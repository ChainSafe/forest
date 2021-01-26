// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use cid::{Cid, Code::Blake2b256};
use clock::ChainEpoch;
use crypto::{DomainSeparationTag, Signature};
use db::MemoryDB;
use encoding::{blake2b_256, de::DeserializeOwned, Cbor};
use fil_types::{
    NetworkVersion, PieceInfo, Randomness, RegisteredSealProof, SealVerifyInfo,
    WindowPoStVerifyInfo,
};
use ipld_blockstore::BlockStore;
use runtime::{ConsensusFault, MessageInfo, Runtime, Syscalls};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::error::Error as StdError;
use vm::{actor_error, ActorError, ExitCode, MethodNum, Serialized, TokenAmount};

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
    pub expect_validate_caller_addr: Option<Vec<Address>>,
    pub expect_validate_caller_type: Option<Vec<Cid>>,
    pub expect_sends: VecDeque<ExpectedMessage>,
    pub expect_create_actor: Option<ExpectCreateActor>,
    pub expect_delete_actor: Option<Address>,
    pub expect_verify_sigs: RefCell<VecDeque<ExpectedVerifySig>>,
    pub expect_verify_seal: RefCell<Option<ExpectVerifySeal>>,
    pub expect_verify_post: RefCell<Option<ExpectVerifyPoSt>>,
    pub expect_compute_unsealed_sector_cid: RefCell<Option<ExpectComputeUnsealedSectorCid>>,
    pub expect_verify_consensus_fault: RefCell<Option<ExpectVerifyConsensusFault>>,
    pub hash_func: Box<dyn Fn(&[u8]) -> [u8; 32]>,
    pub network_version: NetworkVersion,
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
            expect_delete_actor: Default::default(),
            expect_verify_sigs: Default::default(),
            expect_verify_seal: Default::default(),
            expect_verify_post: Default::default(),
            expect_compute_unsealed_sector_cid: Default::default(),
            expect_verify_consensus_fault: Default::default(),
            hash_func: Box::new(|_| [0u8; 32]),
            network_version: NetworkVersion::V0,
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

#[derive(Debug)]
pub struct ExpectedVerifySig {
    pub sig: Signature,
    pub signer: Address,
    pub plaintext: Vec<u8>,
    pub result: Result<(), Box<dyn StdError>>,
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
            return Err(actor_error!(SysErrIllegalArgument; msg));
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
    pub fn get_state<T: Cbor>(&self) -> Result<T, ActorError> {
        // TODO this doesn't handle errors exactly as go implementation
        self.state()
    }

    #[allow(dead_code)]
    pub fn expect_validate_caller_addr(&mut self, addr: Vec<Address>) {
        assert!(addr.len() > 0, "addrs must be non-empty");
        self.expect_validate_caller_addr = Some(addr);
    }

    #[allow(dead_code)]
    pub fn expect_verify_signature(&self, exp: ExpectedVerifySig) {
        self.expect_verify_sigs.borrow_mut().push_back(exp);
    }

    #[allow(dead_code)]
    pub fn set_balance(&mut self, amount: TokenAmount) {
        self.balance = amount;
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
    pub fn expect_validate_caller_type(&mut self, types: Vec<Cid>) {
        assert!(types.len() > 0, "addrs must be non-empty");
        self.expect_validate_caller_type = Some(types);
    }

    #[allow(dead_code)]
    pub fn expect_validate_caller_any(&self) {
        self.expect_validate_caller_any.set(true);
    }

    #[allow(dead_code)]
    pub fn expect_delete_actor(&mut self, beneficiary: Address) {
        self.expect_delete_actor = Some(beneficiary);
    }

    pub fn call(
        &mut self,
        to_code: &Cid,
        method_num: MethodNum,
        params: &Serialized,
    ) -> Result<Serialized, ActorError> {
        self.in_call = true;
        let prev_state = self.state.clone();
        let res = forest_actor::invoke_code(to_code, self, method_num, params)
            .unwrap_or_else(|| Err(actor_error!(SysErrForbidden, "invalid method id")));

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
            self.expect_validate_caller_addr.is_none(),
            "expected ValidateCallerAddr {:?}, not received",
            self.expect_validate_caller_addr
        );
        assert!(
            self.expect_validate_caller_type.is_none(),
            "expected ValidateCallerType {:?}, not received",
            self.expect_validate_caller_type
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
            "expect_verify_consensus_fault not received",
        );

        self.reset();
    }
    pub fn reset(&mut self) {
        self.expect_validate_caller_any.set(false);
        self.expect_validate_caller_addr = None;
        self.expect_validate_caller_type = None;
        self.expect_create_actor = None;
        self.expect_sends.clear();
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

    #[allow(dead_code)]
    pub fn replace_state<C: Cbor>(&mut self, obj: &C) {
        self.state = Some(self.store.put(obj, Blake2b256).unwrap());
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
    fn network_version(&self) -> NetworkVersion {
        self.network_version
    }

    fn message(&self) -> &dyn MessageInfo {
        self.require_in_call();
        self
    }

    fn curr_epoch(&self) -> ChainEpoch {
        self.require_in_call();
        self.epoch
    }

    fn validate_immediate_caller_accept_any(&mut self) -> Result<(), ActorError> {
        self.require_in_call();
        assert!(
            self.expect_validate_caller_any.get(),
            "unexpected validate-caller-any"
        );
        self.expect_validate_caller_any.set(false);
        Ok(())
    }

    fn validate_immediate_caller_is<'a, I>(&mut self, addresses: I) -> Result<(), ActorError>
    where
        I: IntoIterator<Item = &'a Address>,
    {
        self.require_in_call();

        let addrs: Vec<Address> = addresses.into_iter().cloned().collect();

        self.check_argument(addrs.len() > 0, "addrs must be non-empty".to_owned())?;

        assert!(
            self.expect_validate_caller_addr.is_some(),
            "unexpected validate caller addrs"
        );
        assert!(
            &addrs == self.expect_validate_caller_addr.as_ref().unwrap(),
            "unexpected validate caller addrs {:?}, expected {:?}",
            addrs,
            self.expect_validate_caller_addr
        );

        for expected in &addrs {
            if self.message().caller() == expected {
                self.expect_validate_caller_addr = None;
                return Ok(());
            }
        }
        self.expect_validate_caller_addr = None;
        return Err(actor_error!(ErrForbidden;
                "caller address {:?} forbidden, allowed: {:?}",
                self.message().caller(), &addrs
        ));
    }
    fn validate_immediate_caller_type<'a, I>(&mut self, types: I) -> Result<(), ActorError>
    where
        I: IntoIterator<Item = &'a Cid>,
    {
        self.require_in_call();
        let types: Vec<Cid> = types.into_iter().cloned().collect();

        self.check_argument(types.len() > 0, "types must be non-empty".to_owned())?;

        assert!(
            self.expect_validate_caller_type.is_some(),
            "unexpected validate caller code"
        );
        assert!(
            &types == self.expect_validate_caller_type.as_ref().unwrap(),
            "unexpected validate caller code {:?}, expected {:?}",
            types,
            self.expect_validate_caller_type
        );

        for expected in &types {
            if &self.caller_type == expected {
                self.expect_validate_caller_type = None;
                return Ok(());
            }
        }

        self.expect_validate_caller_type = None;

        Err(
            actor_error!(ErrForbidden; "caller type {:?} forbidden, allowed: {:?}",
                self.caller_type, types),
        )
    }

    fn current_balance(&self) -> Result<TokenAmount, ActorError> {
        self.require_in_call();
        Ok(self.balance.clone())
    }

    fn resolve_address(&self, address: &Address) -> Result<Option<Address>, ActorError> {
        self.require_in_call();
        if address.protocol() == address::Protocol::ID {
            return Ok(Some(address.clone()));
        }

        Ok(self.id_addresses.get(&address).cloned())
    }

    fn get_actor_code_cid(&self, addr: &Address) -> Result<Option<Cid>, ActorError> {
        self.require_in_call();

        Ok(self.actor_code_cids.get(&addr).cloned())
    }

    fn get_randomness_from_tickets(
        &self,
        _personalization: DomainSeparationTag,
        _rand_epoch: ChainEpoch,
        _entropy: &[u8],
    ) -> Result<Randomness, ActorError> {
        unimplemented!()
    }

    fn get_randomness_from_beacon(
        &self,
        _personalization: DomainSeparationTag,
        _rand_epoch: ChainEpoch,
        _entropy: &[u8],
    ) -> Result<Randomness, ActorError> {
        unimplemented!()
    }

    fn create<C: Cbor>(&mut self, obj: &C) -> Result<(), ActorError> {
        if self.state.is_some() == true {
            return Err(actor_error!(SysErrIllegalActor; "state already constructed"));
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

    fn transaction<C, RT, F>(&mut self, f: F) -> Result<RT, ActorError>
    where
        C: Cbor,
        F: FnOnce(&mut C, &mut Self) -> Result<RT, ActorError>,
    {
        if self.in_transaction {
            return Err(actor_error!(SysErrIllegalActor; "nested transaction"));
        }
        let mut read_only = self.state()?;
        self.in_transaction = true;
        let ret = f(&mut read_only, self)?;
        self.state = Some(self.put(&read_only).unwrap());
        self.in_transaction = false;
        Ok(ret)
    }

    fn store(&self) -> &MemoryDB {
        &self.store
    }

    fn send(
        &mut self,
        to: Address,
        method: MethodNum,
        params: Serialized,
        value: TokenAmount,
    ) -> Result<Serialized, ActorError> {
        self.require_in_call();
        if self.in_transaction {
            return Err(actor_error!(SysErrIllegalActor; "side-effect within transaction"));
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

        assert!(expected_msg.to == to && expected_msg.method == method && expected_msg.params == params && expected_msg.value == value, "expectedMessage being sent does not match expectation.\nMessage -\t to: {:?} method: {:?} value: {:?} params: {:?}\nExpected -\t {:?}", to, method, value, params, self.expect_sends[0]);

        if value > self.balance {
            return Err(actor_error!(SysErrSenderStateInvalid;
                    "cannot send value: {:?} exceeds balance: {:?}",
                    value, self.balance
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

    fn create_actor(&mut self, code_id: Cid, address: &Address) -> Result<(), ActorError> {
        self.require_in_call();
        if self.in_transaction {
            return Err(actor_error!(SysErrIllegalActor; "side-effect within transaction"));
        }
        let expect_create_actor = self
            .expect_create_actor
            .take()
            .expect("unexpected call to create actor");

        assert!(&expect_create_actor.code_id == &code_id && &expect_create_actor.address == address, "unexpected actor being created, expected code: {:?} address: {:?}, actual code: {:?} address: {:?}", expect_create_actor.code_id, expect_create_actor.address, code_id, address);
        Ok(())
    }

    fn delete_actor(&mut self, addr: &Address) -> Result<(), ActorError> {
        self.require_in_call();
        if self.in_transaction {
            return Err(actor_error!(SysErrIllegalActor; "side-effect within transaction"));
        }
        let exp_act = self.expect_delete_actor.take();
        if exp_act.is_none() {
            panic!("unexpected call to delete actor: {}", addr);
        }
        if exp_act.as_ref().unwrap() != addr {
            panic!(
                "attempt to delete wrong actor. Expected: {}, got: {}",
                exp_act.unwrap(),
                addr
            );
        }
        Ok(())
    }

    fn total_fil_circ_supply(&self) -> Result<TokenAmount, ActorError> {
        unimplemented!();
    }

    fn charge_gas(&mut self, _: &'static str, _: i64) -> Result<(), ActorError> {
        // TODO implement functionality if needed for testing
        Ok(())
    }
}

impl Syscalls for MockRuntime {
    fn verify_signature(
        &self,
        signature: &Signature,
        signer: &Address,
        plaintext: &[u8],
    ) -> Result<(), Box<dyn StdError>> {
        if self.expect_verify_sigs.borrow().is_empty() {
            panic!(
                "Unexpected signature verification sig: {:?}, signer: {}, plaintext: {}",
                signature,
                signer,
                hex::encode(plaintext)
            );
        }
        let exp = self.expect_verify_sigs.borrow_mut().pop_front();
        if let Some(exp) = exp {
            if exp.sig != *signature || exp.signer != *signer || &exp.plaintext[..] != plaintext {
                panic!(
                    "unexpected signature verification\n\
                    sig: {:?}, signer: {}, plaintext: {}\n\
                    expected sig: {:?}, signer: {}, plaintext: {}",
                    signature,
                    signer,
                    hex::encode(plaintext),
                    exp.sig,
                    exp.signer,
                    hex::encode(exp.plaintext)
                )
            }
            exp.result?
        } else {
            panic!(
                "unexpected syscall to verify signature: {:?}, signer: {}, plaintext: {}",
                signature,
                signer,
                hex::encode(plaintext)
            )
        }
        Ok(())
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
            .ok_or(Box::new(actor_error!(ErrIllegalState;
                "Unexpected syscall to ComputeUnsealedSectorCID"
            )))?;

        if exp.reg != reg {
            return Err(Box::new(actor_error!(ErrIllegalState;
                "Unexpected compute_unsealed_sector_cid : reg mismatch"
            )));
        }

        if exp.pieces[..].eq(pieces) {
            return Err(Box::new(actor_error!(ErrIllegalState;
                "Unexpected compute_unsealed_sector_cid : pieces mismatch"
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
        let exp = self.expect_verify_seal.replace(None).ok_or(Box::new(
            actor_error!(ErrIllegalState; "Unexpected syscall to verify seal"),
        ))?;

        if exp.seal != *seal {
            return Err(Box::new(
                actor_error!(ErrIllegalState; "Unexpected seal verification"),
            ));
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
        let exp = self.expect_verify_post.replace(None).ok_or(Box::new(
            actor_error!(ErrIllegalState; "Unexpected syscall to verify PoSt"),
        ))?;

        if exp.post != *post {
            return Err(Box::new(
                actor_error!(ErrIllegalState; "Unexpected PoSt verification"),
            ));
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
            .ok_or(Box::new(
                actor_error!(ErrIllegalState; "Unexpected syscall to verify_consensus_fault"),
            ))?;
        if exp.require_correct_input {
            if exp.block_header_1 != h1 {
                return Err(Box::new(actor_error!(ErrIllegalState; "Header 1 mismatch")));
            }
            if exp.block_header_2 != h2 {
                return Err(Box::new(actor_error!(ErrIllegalState; "Header 2 mismatch")));
            }
            if exp.block_header_extra != extra {
                return Err(Box::new(
                    actor_error!(ErrIllegalState; "Header extra mismatch"),
                ));
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
