// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::gas_block_store::GasBlockStore;
use super::gas_syscalls::GasSyscalls;
use super::gas_tracker::{price_list_by_epoch, GasCharge, GasTracker, PriceList};
use super::Rand;
use actor::*;
use address::{Address, Protocol};
use byteorder::{BigEndian, WriteBytesExt};
use cid::{multihash::Blake2b256, Cid};
use clock::ChainEpoch;
use crypto::DomainSeparationTag;
use fil_types::{DevnetParams, NetworkParams};
use forest_encoding::Cbor;
use forest_encoding::{error::Error as EncodingError, to_vec};
use ipld_blockstore::BlockStore;
use log::warn;
use message::{Message, UnsignedMessage};
use num_bigint::BigInt;
use runtime::{ActorCode, MessageInfo, Runtime, Syscalls};
use state_tree::StateTree;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;
use vm::{
    actor_error, ActorError, ActorState, ExitCode, MethodNum, Randomness, Serialized, TokenAmount,
    EMPTY_ARR_CID, METHOD_SEND,
};

// TODO this param isn't finalized
const ACTOR_EXEC_GAS: GasCharge = GasCharge {
    name: "on_actor_exec",
    compute_gas: 0,
    storage_gas: 0,
};

struct VMMsg {
    caller: Address,
    receiver: Address,
    value_received: TokenAmount,
}

impl MessageInfo for VMMsg {
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

/// Implementation of the Runtime trait.
pub struct DefaultRuntime<'db, 'msg, 'st, 'sys, 'r, BS, SYS, R, P = DevnetParams> {
    state: &'st mut StateTree<'db, BS>,
    store: GasBlockStore<'db, BS>,
    syscalls: GasSyscalls<'sys, SYS>,
    gas_tracker: Rc<RefCell<GasTracker>>,
    message: &'msg UnsignedMessage,
    vm_msg: VMMsg,
    epoch: ChainEpoch,
    origin: Address,
    origin_nonce: u64,
    num_actors_created: u64,
    price_list: PriceList,
    rand: &'r R,
    caller_validated: bool,
    allow_internal: bool,
    params: PhantomData<P>,
}

impl<'db, 'msg, 'st, 'sys, 'r, BS, SYS, R, P>
    DefaultRuntime<'db, 'msg, 'st, 'sys, 'r, BS, SYS, R, P>
where
    BS: BlockStore,
    SYS: Syscalls,
    P: NetworkParams,
    R: Rand,
{
    /// Constructs a new Runtime
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        state: &'st mut StateTree<'db, BS>,
        store: &'db BS,
        syscalls: &'sys SYS,
        gas_used: i64,
        message: &'msg UnsignedMessage,
        epoch: ChainEpoch,
        origin: Address,
        origin_nonce: u64,
        num_actors_created: u64,
        rand: &'r R,
    ) -> Result<Self, ActorError> {
        let price_list = price_list_by_epoch(epoch);
        let gas_tracker = Rc::new(RefCell::new(GasTracker::new(message.gas_limit(), gas_used)));
        let gas_block_store = GasBlockStore {
            price_list: price_list.clone(),
            gas: Rc::clone(&gas_tracker),
            store,
        };
        let gas_syscalls = GasSyscalls {
            price_list: price_list.clone(),
            gas: Rc::clone(&gas_tracker),
            syscalls,
        };

        let caller_id = state
            .lookup_id(&message.from())
            .map_err(|e| actor_error!(fatal("failed to lookup id: {}", e)))?
            .ok_or_else(
                || actor_error!(SysErrInvalidReceiver; "resolve msg from address failed"),
            )?;

        let vm_msg = VMMsg {
            caller: caller_id,
            receiver: *message.receiver(),
            value_received: message.value_received().clone(),
        };

        Ok(DefaultRuntime {
            state,
            store: gas_block_store,
            syscalls: gas_syscalls,
            gas_tracker,
            message,
            vm_msg,
            epoch,
            origin,
            origin_nonce,
            num_actors_created,
            price_list,
            rand,
            allow_internal: true,
            caller_validated: false,
            params: PhantomData,
        })
    }

    /// Adds to amount of used
    /// * Will borrow gas tracker RefCell, do not call if any reference to this exists
    pub fn charge_gas(&mut self, gas: GasCharge) -> Result<(), ActorError> {
        self.gas_tracker.borrow_mut().charge_gas(gas)
    }

    /// Returns gas used by runtime
    /// * Will borrow gas tracker RefCell, do not call if a mutable reference exists
    pub fn gas_used(&self) -> i64 {
        self.gas_tracker.borrow().gas_used()
    }

    fn gas_available(&self) -> i64 {
        self.gas_tracker.borrow().gas_available()
    }

    /// Returns the price list for gas charges within the runtime
    pub fn price_list(&self) -> &PriceList {
        &self.price_list
    }

    /// Get the balance of a particular Actor from their Address
    fn get_balance(&self, addr: &Address) -> Result<BigInt, ActorError> {
        Ok(self
            .state
            .get_actor(&addr)
            .map_err(ActorError::new_fatal)?
            .map(|act| act.balance)
            .unwrap_or_default())
    }

    /// Update the state Cid of the Message receiver
    fn state_commit(&mut self, old_h: &Cid, new_h: Cid) -> Result<(), ActorError> {
        let to_addr = *self.message().receiver();
        let mut actor = self
            .state
            .get_actor(&to_addr)
            .map_err(ActorError::new_fatal)?
            .ok_or_else(|| actor_error!(fatal("failed to get actor to commit state")))?;

        if &actor.state != old_h {
            return Err(actor_error!(fatal(
                "failed to update, inconsistent base reference"
            )));
        }
        actor.state = new_h;
        self.state
            .set_actor(&to_addr, actor)
            .map_err(|e| actor_error!(fatal("failed to set actor in state_commit: {}", e)))?;

        Ok(())
    }

    fn abort_if_already_validated(&mut self) -> Result<(), ActorError> {
        if self.caller_validated {
            Err(actor_error!(SysErrorIllegalActor;
                    "Method must validate caller identity exactly once"))
        } else {
            self.caller_validated = true;
            Ok(())
        }
    }

    /// Helper function for inserting into blockstore.
    fn put<T>(&self, obj: &T) -> Result<Cid, ActorError>
    where
        T: Cbor,
    {
        self.store
            .put(obj, Blake2b256)
            .map_err(|e| match e.downcast::<EncodingError>() {
                Ok(ser_error) => actor_error!(ErrSerialization;
                        "failed to marshal cbor object {}", ser_error),
                Err(other) => actor_error!(fatal("failed to put cbor object: {}", other)),
            })
    }

    /// Helper function for getting deserializable objects from blockstore.
    fn get<T>(&self, cid: &Cid) -> Result<Option<T>, ActorError>
    where
        T: Cbor,
    {
        self.store
            .get(cid)
            .map_err(|e| match e.downcast::<EncodingError>() {
                Ok(ser_error) => actor_error!(ErrSerialization;
                "failed to unmarshal cbor object {}", ser_error),
                Err(other) => actor_error!(fatal("failed to get cbor object: {}", other)),
            })
    }

    fn internal_send(
        &mut self,
        from: Address,
        to: Address,
        method: MethodNum,
        value: TokenAmount,
        params: Serialized,
    ) -> Result<Serialized, ActorError> {
        let msg = UnsignedMessage::builder()
            .from(from)
            .to(to)
            .method_num(method)
            .value(value)
            .params(params)
            .gas_limit(self.gas_available())
            .build()
            .expect("Message creation fails");

        // snapshot state tree
        let snapshot = self
            .state
            .snapshot()
            .map_err(|e| actor_error!(fatal("failed to create snapshot {}", e)))?;

        let send_res = vm_send::<BS, SYS, R, P>(self, &msg, None);
        send_res.map_err(|e| {
            if let Err(e) = self.state.revert_to_snapshot(&snapshot) {
                actor_error!(fatal("failed to revert snapshot: {}", e))
            } else {
                e
            }
        })
    }

    /// creates account actors from only BLS/SECP256K1 addresses.
    pub fn try_create_account_actor(&mut self, addr: &Address) -> Result<ActorState, ActorError> {
        self.charge_gas(self.price_list().on_create_actor())?;

        let addr_id = self
            .state
            .register_new_address(addr)
            .map_err(ActorError::new_fatal)?;

        let act = make_actor(addr)?;

        self.state
            .set_actor(&addr_id, act)
            .map_err(ActorError::new_fatal)?;

        let p = Serialized::serialize(&addr).map_err(|e| {
            actor_error!(fatal(
                "couldn't serialize params for actor construction: {}",
                e
            ))
        })?;

        self.internal_send(
            *SYSTEM_ACTOR_ADDR,
            addr_id,
            account::Method::Constructor as u64,
            TokenAmount::from(0),
            p,
        )?;

        let act = self
            .state
            .get_actor(&addr_id)
            .map_err(ActorError::new_fatal)?
            .ok_or_else(|| actor_error!(fatal("failed to retrieve created actor state")))?;

        Ok(act)
    }
}

impl<BS, SYS, R, P> Runtime<BS> for DefaultRuntime<'_, '_, '_, '_, '_, BS, SYS, R, P>
where
    BS: BlockStore,
    SYS: Syscalls,
    P: NetworkParams,
    R: Rand,
{
    fn message(&self) -> &dyn MessageInfo {
        &self.vm_msg
    }
    fn curr_epoch(&self) -> ChainEpoch {
        self.epoch
    }
    fn validate_immediate_caller_accept_any(&mut self) -> Result<(), ActorError> {
        self.abort_if_already_validated()
    }
    fn validate_immediate_caller_is<'db, I>(&mut self, addresses: I) -> Result<(), ActorError>
    where
        I: IntoIterator<Item = &'db Address>,
    {
        self.abort_if_already_validated()?;

        let imm = self.message().caller();

        // Check if theres is at least one match
        if !addresses.into_iter().any(|a| a == imm) {
            return Err(actor_error!(SysErrForbidden;
                "caller {} is not one of supported", self.message().caller()
            ));
        }
        Ok(())
    }

    fn validate_immediate_caller_type<'db, I>(&mut self, types: I) -> Result<(), ActorError>
    where
        I: IntoIterator<Item = &'db Cid>,
    {
        self.abort_if_already_validated()?;

        let caller_cid = self
            .get_actor_code_cid(self.message().caller())?
            .ok_or_else(|| actor_error!(fatal("failed to lookup code cid for caller")))?;
        if !types.into_iter().any(|c| *c == caller_cid) {
            return Err(actor_error!(SysErrForbidden;
                    "caller cid type {} not one of supported", caller_cid));
        }
        Ok(())
    }

    fn current_balance(&self) -> Result<TokenAmount, ActorError> {
        self.get_balance(self.message.to())
    }

    fn resolve_address(&self, address: &Address) -> Result<Option<Address>, ActorError> {
        self.state
            .lookup_id(&address)
            .map_err(ActorError::new_fatal)
    }

    fn get_actor_code_cid(&self, addr: &Address) -> Result<Option<Cid>, ActorError> {
        Ok(self
            .state
            .get_actor(&addr)
            .map_err(ActorError::new_fatal)?
            .map(|act| act.code))
    }

    fn get_randomness_from_tickets(
        &self,
        personalization: DomainSeparationTag,
        rand_epoch: ChainEpoch,
        entropy: &[u8],
    ) -> Result<Randomness, ActorError> {
        let r = self
            .rand
            .get_chain_randomness(&self.store, personalization, rand_epoch, entropy)
            .map_err(|e| actor_error!(fatal("could not get randomness: {}", e.to_string())))?;

        Ok(Randomness(r))
    }

    fn get_randomness_from_beacon(
        &self,
        personalization: DomainSeparationTag,
        rand_epoch: ChainEpoch,
        entropy: &[u8],
    ) -> Result<Randomness, ActorError> {
        let r = self
            .rand
            .get_chain_randomness(&self.store, personalization, rand_epoch, entropy)
            .map_err(|e| actor_error!(fatal("could not get randomness: {}", e.to_string())))?;

        Ok(Randomness(r))
    }

    fn create<C: Cbor>(&mut self, obj: &C) -> Result<(), ActorError> {
        let c = self.put(obj)?;

        self.state_commit(&EMPTY_ARR_CID, c)
    }

    fn state<C: Cbor>(&self) -> Result<C, ActorError> {
        let actor = self
            .state
            .get_actor(self.message().receiver())
            .map_err(|e| {
                actor_error!(SysErrorIllegalArgument;
                "failed to get actor for Readonly state: {}", e)
            })?
            .ok_or_else(
                || actor_error!(SysErrorIllegalArgument; "Actor readonly state does not exist"),
            )?;

        // TODO revisit as the go impl doesn't handle not exists and nil cases
        self.get(&actor.state)?.ok_or_else(|| {
            actor_error!(fatal(
                "State does not exist for actor state cid: {}",
                actor.state
            ))
        })
    }

    fn transaction<C, RT, F>(&mut self, f: F) -> Result<RT, ActorError>
    where
        C: Cbor,
        F: FnOnce(&mut C, &mut Self) -> RT,
    {
        // get actor
        let act = self.state.get_actor(self.message().receiver())
            .map_err(|e| actor_error!(SysErrorIllegalActor; "failed to get actor for transaction: {}", e))?
            .ok_or_else(|| actor_error!(SysErrorIllegalActor;
                "actor state for transaction doesn't exist"))?;

        // get state for actor based on generic C
        // TODO Lotus is not handling the not exist case, revisit
        let mut state: C = self
            .get(&act.state)?
            .ok_or_else(|| actor_error!(fatal("Actor state does not exist: {}", act.state)))?;

        // Update the state
        self.allow_internal = false;
        let r = f(&mut state, self);
        self.allow_internal = true;

        let c = self.put(&state)?;

        // Committing that change
        self.state_commit(&act.state, c)?;
        Ok(r)
    }

    fn store(&self) -> &BS {
        self.store.store
    }

    fn send(
        &mut self,
        to: Address,
        method: MethodNum,
        params: Serialized,
        value: TokenAmount,
    ) -> Result<Serialized, ActorError> {
        if !self.allow_internal {
            return Err(actor_error!(SysErrorIllegalActor; "runtime.send() is not allowed"));
        }

        let ret = self
            .internal_send(*self.message.receiver(), to, method, value, params)
            .map_err(|e| {
                warn!(
                    "internal send failed: (to: {}) (method: {}) {}",
                    to, method, e
                );
                e
            })?;

        Ok(ret)
    }
    fn new_actor_address(&mut self) -> Result<Address, ActorError> {
        let oa = resolve_to_key_addr(self.state, &self.store, &self.origin)?;
        let mut b = to_vec(&oa).map_err(|e| {
            actor_error!(fatal(
                "Could not serialize address in new_actor_address: {}",
                e
            ))
        })?;
        b.write_u64::<BigEndian>(self.origin_nonce)
            .map_err(|e| actor_error!(fatal("Writing nonce address into a buffer: {}", e)))?;
        b.write_u64::<BigEndian>(self.num_actors_created)
            .map_err(|e| {
                actor_error!(fatal(
                    "Writing number of actors created into a buffer: {}",
                    e
                ))
            })?;
        let addr = Address::new_actor(&b);
        self.num_actors_created += 1;
        Ok(addr)
    }
    fn create_actor(&mut self, code_id: Cid, address: &Address) -> Result<(), ActorError> {
        if !is_builtin_actor(&code_id) {
            return Err(actor_error!(SysErrorIllegalArgument; "Can only create built-in actors."));
        }
        if is_singleton_actor(&code_id) {
            return Err(actor_error!(SysErrorIllegalArgument;
                    "Can only have one instance of singleton actors."));
        }

        if let Ok(Some(_)) = self.state.get_actor(address) {
            return Err(actor_error!(SysErrorIllegalArgument; "Actor address already exists"));
        }

        self.charge_gas(self.price_list.on_create_actor())?;
        self.state
            .set_actor(
                &address,
                ActorState::new(code_id, EMPTY_ARR_CID.clone(), 0.into(), 0),
            )
            .map_err(|e| actor_error!(fatal("creating actor entry: {}", e)))
    }

    /// DeleteActor deletes the executing actor from the state tree, transferring
    /// any balance to beneficiary.
    /// Aborts if the beneficiary does not exist.
    /// May only be called by the actor itself.
    fn delete_actor(&mut self, beneficiary: &Address) -> Result<(), ActorError> {
        self.charge_gas(self.price_list.on_delete_actor())?;
        let balance = self
            .state
            .get_actor(self.message.to())
            .map_err(|e| actor_error!(fatal("failed to get actor {}, {}", self.message.to(), e)))?
            .ok_or_else(
                || actor_error!(SysErrorIllegalActor; "failed to load actor in delete actor"),
            )
            .map(|act| act.balance)?;
        let receiver = *self.message().receiver();
        if balance != 0.into() {
            // Transfer the executing actor's balance to the beneficiary
            transfer(self.state, &receiver, beneficiary, &balance).map_err(|e| {
                actor_error!(fatal(
                    "failed to transfer balance to beneficiary actor: {}",
                    e.msg()
                ))
            })?;
        }

        // Delete the executing actor
        self.state
            .delete_actor(&receiver)
            .map_err(|e| actor_error!(fatal("failed to delete actor: {}", e)))
    }
    fn syscalls(&self) -> &dyn Syscalls {
        &self.syscalls
    }
    fn total_fil_circ_supply(&self) -> Result<TokenAmount, ActorError> {
        let get_actor_state = |addr: &Address| -> Result<ActorState, ActorError> {
            self.state
                .get_actor(&addr)
                .map_err(|e| {
                    actor_error!(ErrIllegalState;
                        "failed to get reward actor for cumputing total supply: {}", e)
                })?
                .ok_or_else(
                    || actor_error!(ErrIllegalState; "Actor address ({}) does not exist", addr),
                )
        };

        let rew = get_actor_state(&REWARD_ACTOR_ADDR)?;
        let burnt = get_actor_state(&BURNT_FUNDS_ACTOR_ADDR)?;
        let market = get_actor_state(&STORAGE_MARKET_ACTOR_ADDR)?;
        let power = get_actor_state(&STORAGE_POWER_ACTOR_ADDR)?;

        let st: power::State = self
            .store
            .get(&power.state)
            .map_err(|e| {
                actor_error!(ErrIllegalState;
                    "failed to get storage power state: {}", e.to_string())
            })?
            .ok_or_else(|| actor_error!(ErrIllegalState; "Failed to retrieve power state"))?;

        let total = P::from_fil(P::TOTAL_FILECOIN)
            - rew.balance
            - market.balance
            - burnt.balance
            - st.total_pledge_collateral;
        Ok(total)
    }
    fn charge_gas(&mut self, name: &'static str, compute: i64) -> Result<(), ActorError> {
        self.charge_gas(GasCharge::new(name, compute, 0))
    }
}

/// Shared logic between the DefaultRuntime and the Interpreter.
/// It invokes methods on different Actors based on the Message.
pub fn vm_send<'db, 'msg, 'st, 'sys, 'r, BS, SYS, R, P>(
    rt: &mut DefaultRuntime<'db, 'msg, 'st, 'sys, 'r, BS, SYS, R, P>,
    msg: &UnsignedMessage,
    gas_cost: Option<GasCharge>,
) -> Result<Serialized, ActorError>
where
    BS: BlockStore,
    SYS: Syscalls,
    P: NetworkParams,
    R: Rand,
{
    if let Some(cost) = gas_cost {
        rt.charge_gas(cost)?;
    }

    // TODO maybe move this
    rt.charge_gas(
        rt.price_list()
            .on_method_invocation(msg.value(), msg.method_num()),
    )?;

    let to_actor = match rt
        .state
        .get_actor(msg.to())
        .map_err(ActorError::new_fatal)?
    {
        Some(act) => act,
        None => {
            // Try to create actor if not exist
            rt.try_create_account_actor(msg.to())?
        }
    };

    if msg.value() > &TokenAmount::from(0) {
        transfer(rt.state, &msg.from(), &msg.to(), &msg.value())
            .map_err(|e| e.wrap("failed to transfer funds"))?;
    }

    if msg.method_num() != METHOD_SEND {
        rt.charge_gas(ACTOR_EXEC_GAS)?;
        return invoke(rt, to_actor.code, msg.method_num(), msg.params(), msg.to());
    }

    Ok(Serialized::default())
}

/// Transfers funds from one Actor to another Actor
fn transfer<BS: BlockStore>(
    state: &mut StateTree<BS>,
    from: &Address,
    to: &Address,
    value: &TokenAmount,
) -> Result<(), ActorError> {
    if from == to {
        return Ok(());
    }

    let from_id = state
        .lookup_id(from)
        .map_err(ActorError::new_fatal)?
        .ok_or_else(|| actor_error!(fatal("Failed to lookup from id for address {}", from)))?;
    let to_id = state
        .lookup_id(to)
        .map_err(ActorError::new_fatal)?
        .ok_or_else(|| actor_error!(fatal("Failed to lookup to id for address {}", to)))?;

    if from_id == to_id {
        return Ok(());
    }

    if value < &0.into() {
        return Err(
            actor_error!(SysErrForbidden; "attempted to transfer negative transfer value {}", value),
        );
    }

    let mut f = state
        .get_actor(&from_id)
        .map_err(ActorError::new_fatal)?
        .ok_or_else(|| {
            actor_error!(fatal(
                "sender actor does not exist in state during transfer"
            ))
        })?;
    let mut t = state
        .get_actor(&to_id)
        .map_err(ActorError::new_fatal)?
        .ok_or_else(|| {
            actor_error!(fatal(
                "receiver actor does not exist in state during transfer"
            ))
        })?;

    f.deduct_funds(&value).map_err(|e| {
        actor_error!(SysErrInsufficientFunds;
        "transfer failed when deducting funds ({}): {}", value, e)
    })?;
    t.deposit_funds(&value);

    state.set_actor(from, f).map_err(ActorError::new_fatal)?;
    state.set_actor(to, t).map_err(ActorError::new_fatal)?;

    Ok(())
}

/// Calls actor code with method and parameters.
fn invoke<'db, 'msg, 'st, 'sys, 'r, BS, SYS, R, P>(
    rt: &mut DefaultRuntime<'db, 'msg, 'st, 'sys, 'r, BS, SYS, R, P>,
    code: Cid,
    method_num: MethodNum,
    params: &Serialized,
    to: &Address,
) -> Result<Serialized, ActorError>
where
    BS: BlockStore,
    SYS: Syscalls,
    P: NetworkParams,
    R: Rand,
{
    match code {
        x if x == *SYSTEM_ACTOR_CODE_ID => system::Actor.invoke_method(rt, method_num, params),
        x if x == *INIT_ACTOR_CODE_ID => init::Actor.invoke_method(rt, method_num, params),
        x if x == *CRON_ACTOR_CODE_ID => cron::Actor.invoke_method(rt, method_num, params),
        x if x == *ACCOUNT_ACTOR_CODE_ID => account::Actor.invoke_method(rt, method_num, params),
        x if x == *POWER_ACTOR_CODE_ID => power::Actor.invoke_method(rt, method_num, params),
        x if x == *MINER_ACTOR_CODE_ID => miner::Actor.invoke_method(rt, method_num, params),
        x if x == *MARKET_ACTOR_CODE_ID => market::Actor.invoke_method(rt, method_num, params),
        x if x == *PAYCH_ACTOR_CODE_ID => paych::Actor.invoke_method(rt, method_num, params),
        x if x == *MULTISIG_ACTOR_CODE_ID => multisig::Actor.invoke_method(rt, method_num, params),
        x if x == *REWARD_ACTOR_CODE_ID => reward::Actor.invoke_method(rt, method_num, params),
        x if x == *VERIFREG_ACTOR_CODE_ID => verifreg::Actor.invoke_method(rt, method_num, params),
        _ => Err(actor_error!(SysErrorIllegalActor; "no code for actor at address {}", to)),
    }
}

/// returns the public key type of address (`BLS`/`SECP256K1`) of an account actor
/// identified by `addr`.
pub fn resolve_to_key_addr<'st, 'bs, BS, S>(
    st: &'st StateTree<'bs, S>,
    store: &'bs BS,
    addr: &Address,
) -> Result<Address, ActorError>
where
    BS: BlockStore,
    S: BlockStore,
{
    if addr.protocol() == Protocol::BLS || addr.protocol() == Protocol::Secp256k1 {
        return Ok(*addr);
    }

    let act = st
        .get_actor(&addr)
        .map_err(|e| actor_error!(SysErrInternal; e))?
        .ok_or_else(|| actor_error!(SysErrInternal; "Failed to retrieve actor: {}", addr))?;

    if act.code != *ACCOUNT_ACTOR_CODE_ID {
        return Err(actor_error!(fatal(
            "Address was not found for an account actor: {}",
            addr
        )));
    }
    let acc_st: account::State = store
        .get(&act.state)
        .map_err(|e| {
            actor_error!(fatal(
                "Failed to get account actor state for: {}, e: {}",
                addr,
                e
            ))
        })?
        .ok_or_else(|| {
            actor_error!(fatal(
                "Address was not found for an account actor: {}",
                addr
            ))
        })?;

    Ok(acc_st.address)
}

fn make_actor(addr: &Address) -> Result<ActorState, ActorError> {
    match addr.protocol() {
        Protocol::BLS => Ok(new_bls_account_actor()),
        Protocol::Secp256k1 => Ok(new_secp256k1_account_actor()),
        Protocol::ID => {
            Err(actor_error!(SysErrInvalidReceiver; "no actor with given id: {}", addr))
        }
        Protocol::Actor => Err(actor_error!(SysErrInvalidReceiver; "no such actor: {}", addr)),
    }
}

fn new_bls_account_actor() -> ActorState {
    ActorState {
        code: ACCOUNT_ACTOR_CODE_ID.clone(),
        balance: TokenAmount::from(0),
        state: EMPTY_ARR_CID.clone(),
        sequence: 0,
    }
}

fn new_secp256k1_account_actor() -> ActorState {
    ActorState {
        code: ACCOUNT_ACTOR_CODE_ID.clone(),
        balance: TokenAmount::from(0),
        state: EMPTY_ARR_CID.clone(),
        sequence: 0,
    }
}
