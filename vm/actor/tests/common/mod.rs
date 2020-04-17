// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::{
    self, ACCOUNT_ACTOR_CODE_ID, CRON_ACTOR_CODE_ID, INIT_ACTOR_CODE_ID, MARKET_ACTOR_CODE_ID,
    MINER_ACTOR_CODE_ID, MULTISIG_ACTOR_CODE_ID, PAYCH_ACTOR_CODE_ID, POWER_ACTOR_CODE_ID,
    REWARD_ACTOR_CODE_ID, SYSTEM_ACTOR_CODE_ID,
};
use address::Address;
use cid::{multihash::Blake2b256, Cid};
use clock::ChainEpoch;
use crypto::DomainSeparationTag;
use encoding::{de::DeserializeOwned, Cbor};
use ipld_blockstore::BlockStore;
use message::UnsignedMessage;
use runtime::{ActorCode, Runtime, Syscalls};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use vm::{ActorError, ExitCode, MethodNum, Randomness, Serialized, TokenAmount};

pub struct MockRuntime<'a, BS: BlockStore> {
    pub epoch: ChainEpoch,
    pub receiver: Address,
    pub caller: Address,
    pub caller_type: Cid,
    pub miner: Address,
    pub value_received: TokenAmount,
    pub id_addresses: HashMap<Address, Address>,
    pub actor_code_cids: HashMap<Address, Cid>,
    pub new_actor_addr: Address,

    // syscalls: syscaller

    // Actor State
    pub state: Option<Cid>,
    pub balance: TokenAmount,

    // VM Impl
    pub in_call: bool,
    pub store: &'a BS,
    pub in_transaction: bool,

    // Expectations
    pub expect_validate_caller_any: Cell<bool>,
    pub expect_validate_caller_addr: RefCell<Option<Vec<Address>>>,
    pub expect_validate_caller_type: RefCell<Option<Vec<Cid>>>,
    pub expect_sends: VecDeque<ExpectedMessage>,
    pub expect_create_actor: Option<ExpectCreateActor>,
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

impl<'a, BS: BlockStore> MockRuntime<'a, BS> {
    pub fn new(bs: &'a BS, receiver: Address) -> Self {
        Self {
            epoch: 0,
            receiver: receiver,
            caller: Address::default(),
            caller_type: Cid::default(),

            miner: Address::default(),
            value_received: 0u8.into(),

            id_addresses: HashMap::new(),
            actor_code_cids: HashMap::new(),
            new_actor_addr: Address::default(),

            state: None,
            balance: 0u8.into(),

            // VM Impl
            in_call: false,
            store: bs,
            in_transaction: false,

            // Expectations
            expect_validate_caller_any: Cell::new(false),
            expect_validate_caller_addr: RefCell::new(None),
            expect_validate_caller_type: RefCell::new(None),
            expect_sends: VecDeque::new(),
            expect_create_actor: None,
        }
    }
    fn require_in_call(&self) {
        self.require(
            self.in_call,
            "invalid runtime invocation outside of method call",
        )
    }
    fn require(&self, predicate: bool, msg: &'static str) {
        assert!(predicate, msg)
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
    pub fn get_state<T: DeserializeOwned>(&self) -> Result<T, ActorError> {
        let data: T = self
            .store
            .get(&self.state.as_ref().unwrap())
            .unwrap()
            .unwrap();
        Ok(data)
    }
    pub fn expect_validate_caller_addr(&self, addr: &[Address]) {
        self.require(addr.len() > 0, "addrs must be non-empty");
        *self.expect_validate_caller_addr.borrow_mut() = Some(addr.to_vec());
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
                actor::cron::Actor.invoke_method(self, method_num, params)
            }
            x if x == &*REWARD_ACTOR_CODE_ID => {
                actor::cron::Actor.invoke_method(self, method_num, params)
            }
            _ => Err(ActorError::new(
                ExitCode::SysErrForbidden,
                "invalid method id".to_owned(),
            )),
        };
        self.in_call = false;
        return res;
    }
    pub fn verify(&mut self) {
        if self.expect_validate_caller_any.get() == true {
            panic!("expected ValidateCallerAny, not received")
        }
        if self.expect_validate_caller_addr.borrow().as_ref().is_some() {
            panic!(
                "expected ValidateCallerAddr {:?}, not received",
                self.expect_validate_caller_addr.borrow().as_ref().unwrap()
            )
        }
        if self.expect_validate_caller_type.borrow().as_ref().is_some() {
            panic!(
                "expected ValidateCallerType {:?}, not received",
                self.expect_validate_caller_type.borrow().as_ref().unwrap()
            )
        }
        if self.expect_sends.len() > 0 {
            panic!(
                "expected all message to be send, unsent messages {:?}",
                self.expect_sends
            )
        }
        if self.expect_create_actor.is_some() {
            panic!(
                "expected actor to be created, uncreated actor: {:?}",
                self.expect_create_actor
            )
        }

        self.reset();
    }
    pub fn reset(&mut self) {
        self.expect_validate_caller_any.set(false);
        *self.expect_validate_caller_addr.borrow_mut() = None;
        *self.expect_validate_caller_type.borrow_mut() = None;
        self.expect_create_actor = None;
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
}

impl<BS: BlockStore> Runtime<BS> for MockRuntime<'_, BS> {
    // impl<ST: StateTree, BS: BlockStore> Runtime<BS> for MockRuntime<'_, ST, BS> {
    fn message(&self) -> &UnsignedMessage {
        self.require_in_call();
        todo!();
    }

    fn curr_epoch(&self) -> ChainEpoch {
        self.require_in_call();
        self.epoch
    }

    fn validate_immediate_caller_accept_any(&self) {
        self.require_in_call();
        if !self.expect_validate_caller_any.get() {
            panic!("unexpected validate-caller-any");
        }
        self.expect_validate_caller_any.set(false);
    }
    fn validate_immediate_caller_is<'a, I>(&self, addresses: I) -> Result<(), ActorError>
    where
        I: IntoIterator<Item = &'a Address>,
    {
        self.require_in_call();

        let addrs: Vec<Address> = addresses.into_iter().cloned().collect();
        let mut expect_validate_caller_addr = self.expect_validate_caller_addr.borrow_mut();
        let is_expect_validate_caller_addr = expect_validate_caller_addr.is_some();

        self.check_argument(addrs.len() > 0, "addrs must be non-empty".to_owned())?;

        assert!(
            is_expect_validate_caller_addr,
            "unexpected validate caller addrs"
        );
        assert!(
            !expect_validate_caller_addr.as_ref().unwrap().is_empty(),
            "unexpected validate caller addrs"
        );
        assert!(
            &addrs == expect_validate_caller_addr.as_ref().unwrap(),
            "unexpected validate caller addrs {:?}, expected {:?}",
            addrs,
            expect_validate_caller_addr.as_ref()
        );

        for expected in &addrs {
            if &self.caller == expected {
                *expect_validate_caller_addr = None;
                return Ok(());
            }
        }
        *expect_validate_caller_addr = None;
        return Err(ActorError::new(
            ExitCode::ErrForbidden,
            format!(
                "caller address {:?} forbidden, allowed: {:?}",
                self.caller, &addrs
            ),
        ));
    }
    fn validate_immediate_caller_type<'a, I>(&self, types: I) -> Result<(), ActorError>
    where
        I: IntoIterator<Item = &'a Cid>,
    {
        self.require_in_call();
        let types: Vec<Cid> = types.into_iter().cloned().collect();

        let mut expect_validate_caller_type = self.expect_validate_caller_type.borrow_mut();
        let is_expect_validate_caller_type = expect_validate_caller_type.is_some();

        self.check_argument(types.len() > 0, "types must be non-empty".to_owned())?;

        assert!(
            is_expect_validate_caller_type,
            "unexpected validate caller code"
        );
        assert!(
            !expect_validate_caller_type.as_ref().unwrap().is_empty(),
            "unexpected validate caller code"
        );
        assert!(
            &types != expect_validate_caller_type.as_ref().unwrap(),
            "unexpected validate caller code {:?}, expected {:?}",
            types,
            self.expect_validate_caller_type
        );

        for expected in &types {
            if &self.caller_type == expected {
                *expect_validate_caller_type = None;
                return Ok(());
            }
        }

        *expect_validate_caller_type = None;

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
        let resolved = self.id_addresses.get(&address).unwrap();
        return Ok(resolved.clone());
    }

    fn get_actor_code_cid(&self, addr: &Address) -> Result<Cid, ActorError> {
        self.require_in_call();
        let ret = self.actor_code_cids.get(&addr).unwrap();
        Ok(ret.clone())
    }

    fn get_randomness(
        _personalization: DomainSeparationTag,
        _rand_epoch: ChainEpoch,
        _entropy: &[u8],
    ) -> Randomness {
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
        F: FnOnce(&mut C, &Self) -> R,
    {
        if self.in_transaction {
            return Err(self.abort(ExitCode::SysErrorIllegalActor, "nested transaction"));
        }
        let mut read_only = self.state()?;
        self.in_transaction = true;
        let ret = f(&mut read_only, &self);
        self.state = Some(self.put(&read_only).unwrap());
        Ok(ret)
    }

    fn store(&self) -> &BS {
        self.store
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

        let expected_msg = self.expect_sends[0].clone();

        if &expected_msg.to != to
            || expected_msg.method != method
            || &expected_msg.params != params
            || &expected_msg.value != value
        {
            panic!("expectedMessage being sent does not match expectation.\nMessage -\t to: {:?} method: {:?} value: {:?} params: {:?}\nExpected -\t {:?}", to, method, value, params, &self.expect_sends[0])
        }
        if value > &self.balance {
            return Err(self.abort(
                ExitCode::SysErrSenderStateInvalid,
                format!(
                    "cannot send value: {:?} exceeds balance: {:?}",
                    value, self.balance
                ),
            ));
        }
        let _ = self.expect_sends.pop_front();
        self.balance -= value;

        return Ok(expected_msg.send_return);
    }

    fn abort<S: AsRef<str>>(&self, exit_code: ExitCode, msg: S) -> ActorError {
        ActorError::new(exit_code, msg.as_ref().to_owned())
    }

    fn new_actor_address(&mut self) -> Result<Address, ActorError> {
        self.require_in_call();
        // TODO: This is supposed to be the Undef addr. We have no impled that in our default.
        if self.new_actor_addr == Address::default() {
            panic!("unexpected call to new actor address");
        }
        let ret = self.new_actor_addr.clone();
        // TODO: This is supposed to be the Undef addr. We have no impled that in our default.
        self.new_actor_addr = Address::default();
        return Ok(ret);
    }

    fn create_actor(&mut self, code_id: &Cid, address: &Address) -> Result<(), ActorError> {
        self.require_in_call();
        let is_expect_create_actor = self.expect_create_actor.is_none();
        if self.in_transaction {
            return Err(self.abort(
                ExitCode::SysErrorIllegalActor,
                "side-effect within transaction".to_owned(),
            ));
        }
        if is_expect_create_actor {
            panic!("unexpected call to create actor");
        }
        let expect_create_actor = self.expect_create_actor.clone().unwrap();
        if &expect_create_actor.code_id != code_id || &expect_create_actor.address != address {
            panic!("unexpected actor being created, expected code: {:?} address: {:?}, actual code: {:?} address: {:?}", expect_create_actor.code_id, expect_create_actor.address, code_id, address)
        }
        self.expect_create_actor = None;
        Ok(())
    }

    fn delete_actor(&mut self) -> Result<(), ActorError> {
        self.require_in_call();
        if self.in_transaction {
            return Err(self.abort(
                ExitCode::SysErrorIllegalActor,
                "side-effect within transaction".to_owned(),
            ));
        }
        todo!("implement me???")
    }

    fn syscalls(&self) -> &dyn Syscalls {
        unimplemented!()
    }
}
