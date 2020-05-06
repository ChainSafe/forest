// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::gas_block_store::GasBlockStore;
use super::gas_syscalls::GasSyscalls;
use actor::{
    self, account, ACCOUNT_ACTOR_CODE_ID, CRON_ACTOR_CODE_ID, INIT_ACTOR_CODE_ID,
    MARKET_ACTOR_CODE_ID, MINER_ACTOR_CODE_ID, MULTISIG_ACTOR_CODE_ID, PAYCH_ACTOR_CODE_ID,
    POWER_ACTOR_CODE_ID, REWARD_ACTOR_CODE_ID, SYSTEM_ACTOR_CODE_ID,
};
use address::{Address, Protocol};
use byteorder::{BigEndian, WriteBytesExt};
use cid::{multihash::Blake2b256, Cid};
use clock::ChainEpoch;
use crypto::DomainSeparationTag;
use forest_encoding::to_vec;
use forest_encoding::Cbor;
use ipld_blockstore::BlockStore;
use message::{Message, UnsignedMessage};
use num_bigint::BigUint;
use runtime::{ActorCode, Runtime, Syscalls};
use state_tree::StateTree;
use std::cell::RefCell;
use std::rc::Rc;
use vm::{
    price_list_by_epoch, ActorError, ActorState, ExitCode, GasTracker, MethodNum, PriceList,
    Randomness, Serialized, TokenAmount, METHOD_SEND,
};

/// Implementation of the Runtime trait.
pub struct DefaultRuntime<'db, 'msg, 'st, 'sys, BS, SYS> {
    state: &'st mut StateTree<'db, BS>,
    store: GasBlockStore<'db, BS>,
    syscalls: GasSyscalls<'sys, SYS>,
    gas_tracker: Rc<RefCell<GasTracker>>,
    message: &'msg UnsignedMessage,
    epoch: ChainEpoch,
    origin: Address,
    origin_nonce: u64,
    num_actors_created: u64,
    price_list: PriceList,
}

impl<'db, 'msg, 'st, 'sys, BS, SYS> DefaultRuntime<'db, 'msg, 'st, 'sys, BS, SYS>
where
    BS: BlockStore,
    SYS: Syscalls,
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
    ) -> Self {
        let price_list = price_list_by_epoch(epoch);
        let gas_tracker = Rc::new(RefCell::new(GasTracker::new(
            message.gas_limit() as i64,
            gas_used,
        )));
        let gas_block_store = GasBlockStore {
            price_list,
            gas: Rc::clone(&gas_tracker),
            store,
        };
        let gas_syscalls = GasSyscalls {
            price_list,
            gas: Rc::clone(&gas_tracker),
            syscalls,
        };
        DefaultRuntime {
            state,
            store: gas_block_store,
            syscalls: gas_syscalls,
            gas_tracker,
            message,
            epoch,
            origin,
            origin_nonce,
            num_actors_created,
            price_list,
        }
    }

    /// Adds to amount of used
    /// * Will borrow gas tracker RefCell, do not call if any reference to this exists
    pub fn charge_gas(&mut self, to_use: i64) -> Result<(), ActorError> {
        self.gas_tracker.borrow_mut().charge_gas(to_use)
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
    pub fn price_list(&self) -> PriceList {
        self.price_list
    }

    /// Gets the specified Actor from the state tree
    fn get_actor(&self, addr: &Address) -> Result<ActorState, ActorError> {
        // TODO handle exit codes specifically, this leads to a broken implementation
        self.state
            .get_actor(&addr)
            .map_err(|e| {
                self.abort(
                    ExitCode::SysErrInternal,
                    format!("failed to load actor: {}", e),
                )
            })?
            .ok_or_else(|| self.abort(ExitCode::SysErrInternal, "actor not found"))
    }

    /// Get the balance of a particular Actor from their Address
    fn get_balance(&self, addr: &Address) -> Result<BigUint, ActorError> {
        // TODO fix this, not found should return 0 not error, on error should turn error into fatal
        self.get_actor(&addr).map(|act| act.balance)
    }

    /// Update the state Cid of the Message receiver
    fn state_commit(&mut self, old_h: &Cid, new_h: Cid) -> Result<(), ActorError> {
        let to_addr = *self.message().to();
        let mut actor = self.get_actor(&to_addr)?;

        if &actor.state != old_h {
            return Err(self.abort(
                ExitCode::ErrIllegalState,
                "failed to update, inconsistent base reference".to_owned(),
            ));
        }
        actor.state = new_h;
        self.state.set_actor(&to_addr, actor).map_err(|e| {
            self.abort(
                ExitCode::SysErrInternal,
                format!("failed to set actor in state_commit: {}", e),
            )
        })?;

        Ok(())
    }
}

impl<BS, SYS> Runtime<BS> for DefaultRuntime<'_, '_, '_, '_, BS, SYS>
where
    BS: BlockStore,
    SYS: Syscalls,
{
    fn message(&self) -> &UnsignedMessage {
        &self.message
    }
    fn curr_epoch(&self) -> ChainEpoch {
        self.epoch
    }
    fn validate_immediate_caller_accept_any(&self) {}
    fn validate_immediate_caller_is<'db, I>(&self, addresses: I) -> Result<(), ActorError>
    where
        I: IntoIterator<Item = &'db Address>,
    {
        let imm = self.resolve_address(self.message().from())?;

        // Check if theres is at least one match
        if !addresses.into_iter().any(|a| *a == imm) {
            return Err(self.abort(
                ExitCode::SysErrForbidden,
                format!("caller is not one of {}", self.message().from()),
            ));
        }
        Ok(())
    }

    fn validate_immediate_caller_type<'db, I>(&self, types: I) -> Result<(), ActorError>
    where
        I: IntoIterator<Item = &'db Cid>,
    {
        let caller_cid = self.get_actor_code_cid(self.message().to())?;
        if types.into_iter().any(|c| *c == caller_cid) {
            return Err(self.abort(
                ExitCode::SysErrForbidden,
                format!(
                    "caller cid type {} one of {}",
                    caller_cid,
                    self.message().from()
                ),
            ));
        }
        Ok(())
    }
    fn current_balance(&self) -> Result<TokenAmount, ActorError> {
        self.get_balance(self.message.to())
    }
    fn resolve_address(&self, address: &Address) -> Result<Address, ActorError> {
        self.state
            .lookup_id(&address)
            .map_err(|e| self.abort(ExitCode::ErrPlaceholder, e))
    }
    fn get_actor_code_cid(&self, addr: &Address) -> Result<Cid, ActorError> {
        self.get_actor(&addr).map(|act| act.code)
    }
    fn get_randomness(
        _personalization: DomainSeparationTag,
        _rand_epoch: ChainEpoch,
        _entropy: &[u8],
    ) -> Randomness {
        todo!()
    }

    fn create<C: Cbor>(&mut self, obj: &C) -> Result<(), ActorError> {
        let c = self.store.put(obj, Blake2b256).map_err(|e| {
            self.abort(
                ExitCode::ErrPlaceholder,
                format!("storage put in create: {}", e.to_string()),
            )
        })?;
        // TODO: This is almost certainly wrong. Need to CBOR an empty slice and calculate Cid
        self.state_commit(&Cid::default(), c)
    }
    fn state<C: Cbor>(&self) -> Result<C, ActorError> {
        let actor = self.get_actor(self.message().to())?;
        self.store
            .get(&actor.state)
            .map_err(|e| {
                self.abort(
                    ExitCode::ErrPlaceholder,
                    format!("storage get error in read only state: {}", e.to_string()),
                )
            })?
            .ok_or_else(|| {
                self.abort(
                    ExitCode::ErrPlaceholder,
                    "storage get error in read only state".to_owned(),
                )
            })
    }

    fn transaction<C, R, F>(&mut self, f: F) -> Result<R, ActorError>
    where
        C: Cbor,
        F: FnOnce(&mut C, &Self) -> R,
    {
        // get actor
        let act = self.get_actor(self.message().to())?;

        // get state for actor based on generic C
        let mut state: C = self
            .store
            .get(&act.state)
            .map_err(|e| {
                self.abort(
                    ExitCode::ErrPlaceholder,
                    format!("storage get error in transaction: {}", e.to_string()),
                )
            })?
            .ok_or_else(|| {
                self.abort(
                    ExitCode::ErrPlaceholder,
                    "storage get error in transaction".to_owned(),
                )
            })?;

        // Update the state
        let r = f(&mut state, &self);

        let c = self.store.put(&state, Blake2b256).map_err(|e| {
            self.abort(
                ExitCode::ErrPlaceholder,
                format!("storage put in create: {}", e.to_string()),
            )
        })?;

        // Committing that change
        self.state_commit(&act.state, c)?;
        Ok(r)
    }

    fn store(&self) -> &BS {
        self.store.store
    }

    fn send(
        &mut self,
        to: &Address,
        method: MethodNum,
        params: &Serialized,
        value: &TokenAmount,
    ) -> Result<Serialized, ActorError> {
        let msg = UnsignedMessage::builder()
            .to(*to)
            .from(*self.message.from())
            .method_num(method)
            .value(value.clone())
            .gas_limit(self.gas_available() as u64)
            .params(params.clone())
            .build()
            .unwrap();

        // snapshot state tree
        let snapshot = self
            .state
            .snapshot()
            .map_err(|_e| self.abort(ExitCode::ErrPlaceholder, "failed to create snapshot"))?;

        let epoch = self.curr_epoch();
        let send_res = {
            let mut parent = DefaultRuntime::new(
                self.state,
                self.store.store,
                self.syscalls.syscalls,
                self.gas_used(),
                &msg,
                epoch,
                self.origin,
                self.origin_nonce,
                self.num_actors_created,
            );
            internal_send::<BS, SYS>(&mut parent, &msg, 0)
        };
        if send_res.is_err() {
            self.state
                .revert_to_snapshot(&snapshot)
                .map_err(|_e| self.abort(ExitCode::ErrPlaceholder, "failed to revert snapshot"))?;
        }
        send_res
    }

    fn abort<S: AsRef<str>>(&self, exit_code: ExitCode, msg: S) -> ActorError {
        ActorError::new(exit_code, msg.as_ref().to_owned())
    }
    fn new_actor_address(&mut self) -> Result<Address, ActorError> {
        let oa = resolve_to_key_addr(self.state, &self.store, &self.origin)?;
        let mut b = to_vec(&oa).map_err(|e| {
            self.abort(
                ExitCode::ErrSerialization,
                format!("Could not serialize address in new_actor_address: {}", e),
            )
        })?;
        b.write_u64::<BigEndian>(self.origin_nonce).map_err(|e| {
            self.abort(
                ExitCode::ErrSerialization,
                format!("Writing nonce address into a buffer: {}", e.to_string()),
            )
        })?;
        b.write_u64::<BigEndian>(self.num_actors_created)
            .map_err(|e| {
                self.abort(
                    ExitCode::ErrSerialization,
                    format!(
                        "Writing number of actors created into a buffer: {}",
                        e.to_string()
                    ),
                )
            })?;
        let addr = Address::new_actor(&b);
        self.num_actors_created += 1;
        Ok(addr)
    }
    fn create_actor(&mut self, code_id: &Cid, address: &Address) -> Result<(), ActorError> {
        self.charge_gas(self.price_list.on_create_actor())?;
        self.state
            .set_actor(
                &address,
                ActorState::new(code_id.clone(), Cid::default(), 0u64.into(), 0),
            )
            .map_err(|e| {
                self.abort(
                    ExitCode::SysErrInternal,
                    format!("creating actor entry: {}", e),
                )
            })
    }
    fn delete_actor(&mut self) -> Result<(), ActorError> {
        self.charge_gas(self.price_list.on_delete_actor())?;
        let balance = self.get_actor(self.message.to()).map(|act| act.balance)?;
        if !balance.eq(&0u64.into()) {
            return Err(self.abort(
                ExitCode::SysErrInternal,
                "cannot delete actor with non-zero balance",
            ));
        }
        self.state.delete_actor(self.message.to()).map_err(|e| {
            self.abort(
                ExitCode::SysErrInternal,
                format!("failed to delete actor: {}", e),
            )
        })
    }
    fn syscalls(&self) -> &dyn Syscalls {
        &self.syscalls
    }
}
/// Shared logic between the DefaultRuntime and the Interpreter.
/// It invokes methods on different Actors based on the Message.
pub fn internal_send<BS, SYS>(
    runtime: &mut DefaultRuntime<'_, '_, '_, '_, BS, SYS>,
    msg: &UnsignedMessage,
    _gas_cost: i64,
) -> Result<Serialized, ActorError>
where
    BS: BlockStore,
    SYS: Syscalls,
{
    runtime.charge_gas(
        runtime
            .price_list()
            .on_method_invocation(msg.value(), msg.method_num()),
    )?;

    // TODO: we need to try to recover here and try to create account actor
    let to_actor = runtime.get_actor(msg.to())?;

    if msg.value() != &0u8.into() {
        transfer(runtime.state, &msg.from(), &msg.to(), &msg.value())
            .map_err(|e| ActorError::new(ExitCode::SysErrSenderInvalid, e))?;
    }

    let method_num = msg.method_num();

    if method_num != METHOD_SEND {
        let ret = {
            // TODO: make its own method/struct
            match to_actor.code {
                x if x == *SYSTEM_ACTOR_CODE_ID => {
                    actor::system::Actor.invoke_method(runtime, method_num, msg.params())
                }
                x if x == *INIT_ACTOR_CODE_ID => {
                    actor::init::Actor.invoke_method(runtime, method_num, msg.params())
                }
                x if x == *CRON_ACTOR_CODE_ID => {
                    actor::cron::Actor.invoke_method(runtime, method_num, msg.params())
                }
                x if x == *ACCOUNT_ACTOR_CODE_ID => {
                    actor::account::Actor.invoke_method(runtime, method_num, msg.params())
                }
                x if x == *POWER_ACTOR_CODE_ID => {
                    actor::power::Actor.invoke_method(runtime, method_num, msg.params())
                }
                x if x == *MINER_ACTOR_CODE_ID => {
                    actor::miner::Actor.invoke_method(runtime, method_num, msg.params())
                }
                x if x == *MARKET_ACTOR_CODE_ID => {
                    actor::market::Actor.invoke_method(runtime, method_num, msg.params())
                }
                x if x == *PAYCH_ACTOR_CODE_ID => {
                    actor::paych::Actor.invoke_method(runtime, method_num, msg.params())
                }
                x if x == *MULTISIG_ACTOR_CODE_ID => {
                    actor::cron::Actor.invoke_method(runtime, method_num, msg.params())
                }
                x if x == *REWARD_ACTOR_CODE_ID => {
                    actor::cron::Actor.invoke_method(runtime, method_num, msg.params())
                }
                _ => Err(ActorError::new(
                    ExitCode::SysErrorIllegalActor,
                    format!("no code for actor at address {}", msg.to()),
                )),
            }
        };
        return ret;
    }
    Ok(Serialized::default())
}

/// Transfers funds from one Actor to another Actor
fn transfer<BS: BlockStore>(
    state: &mut StateTree<BS>,
    from: &Address,
    to: &Address,
    value: &TokenAmount,
) -> Result<(), String> {
    if from == to {
        return Ok(());
    }
    if value < &0u8.into() {
        return Err("Negative transfer value".to_owned());
    }

    let mut f = state
        .get_actor(from)?
        .ok_or("Transfer failed when retrieving sender actor")?;
    let mut t = state
        .get_actor(to)?
        .ok_or("Transfer failed when retrieving receiver actor")?;

    f.deduct_funds(&value)?;
    t.deposit_funds(&value);

    state.set_actor(from, f)?;
    state.set_actor(to, t)?;

    Ok(())
}

/// Returns public address of the specified actor address
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
        .map_err(|e| ActorError::new(ExitCode::SysErrInternal, e))?
        .ok_or_else(|| {
            ActorError::new(
                ExitCode::SysErrInternal,
                format!("Failed to retrieve actor: {}", addr),
            )
        })?;

    if act.code != *ACCOUNT_ACTOR_CODE_ID {
        return Err(ActorError::new_fatal(format!(
            "Address was not found for an account actor: {}",
            addr
        )));
    }
    let acc_st: account::State = store
        .get(&act.state)
        .map_err(|e| {
            ActorError::new_fatal(format!(
                "Failed to get account actor state for: {}, e: {}",
                addr, e
            ))
        })?
        .ok_or_else(|| {
            ActorError::new_fatal(format!(
                "Address was not found for an account actor: {}",
                addr
            ))
        })?;

    Ok(acc_st.address)
}
