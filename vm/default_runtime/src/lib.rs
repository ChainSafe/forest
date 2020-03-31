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
use forest_encoding::Cbor;
use ipld_blockstore::BlockStore;
use message::{Message, UnsignedMessage};
use num_bigint::BigUint;
use runtime::{ActorCode, Runtime};
use vm::{
    ActorError, ActorState, ExitCode, MethodNum, Randomness, Serialized, StateTree, TokenAmount,
    METHOD_SEND,
};
pub const PLACEHOLDER_GAS: u64 = 1;

pub struct DefaultRuntime<'a, 'b, 'c, ST: StateTree, BS: BlockStore> {
    state: &'c mut ST,
    store: &'a BS,
    gas_used: u64,
    gas_available: u64,
    message: &'b UnsignedMessage,
    epoch: ChainEpoch,
    origin: Address,
    origin_nonce: u64,
}

impl<'a, 'b, 'c, ST: StateTree, BS: BlockStore> DefaultRuntime<'a, 'b, 'c, ST, BS> {
    pub fn new(
        state: &'c mut ST,
        store: &'a BS,
        gas_used: u64,
        message: &'b UnsignedMessage,
        epoch: ChainEpoch,
        origin: &Address,
        origin_nonce: u64,
    ) -> Self {
        DefaultRuntime {
            state,
            store,
            gas_used,
            gas_available: message.gas_limit(),
            message,
            epoch,
            origin: origin.clone(),
            origin_nonce,
        }
    }

    pub fn charge_gas(&mut self, to_use: u64) {
        self.gas_used += to_use;
    }

    pub fn get_actor(&self, addr: &Address) -> Result<ActorState, ActorError> {
        self.state
            .get_actor(&addr)
            .map_err(|e| {
                self.abort(
                    ExitCode::SysErrInternal,
                    format!("failed to load actor: {}", e),
                )
            })
            .and_then(|act| {
                act.ok_or_else(|| self.abort(ExitCode::SysErrInternal, "actor not found"))
            })
    }

    pub fn get_balance(&self, addr: &Address) -> Result<BigUint, ActorError> {
        self.get_actor(&addr).map(|act| act.balance)
    }

    fn state_commit(&mut self, old_h: &Cid, new_h: &Cid) -> Result<(), ActorError> {
        let to_addr = self.message().to().clone();
        let mut actor = self.get_actor(&to_addr)?;

        if &actor.state != old_h {
            return Err(self.abort(
                ExitCode::ErrIllegalState,
                "failed to update, inconsistent base reference".to_owned(),
            ));
        }
        actor.state = new_h.clone();
        self.state.set_actor(&to_addr, actor).map_err(|e| {
            self.abort(
                ExitCode::SysErrInternal,
                format!("failed to set actor in state_commit: {}", e),
            )
        })?;

        Ok(())
    }
}

impl<ST: StateTree, BS: BlockStore> Runtime<BS> for DefaultRuntime<'_, '_, '_, ST, BS> {
    fn message(&self) -> &UnsignedMessage {
        &self.message
    }
    fn curr_epoch(&self) -> ChainEpoch {
        self.epoch
    }
    fn validate_immediate_caller_accept_any(&self) {
        return;
    }
    fn validate_immediate_caller_is<'a, I>(&self, addresses: I) -> Result<(), ActorError>
    where
        I: Iterator<Item = &'a Address>,
    {
        let imm = self.resolve_address(self.message().from())?;

        // Check if theres is at least one match
        match addresses.filter(|a| **a == imm).next() {
            Some(_) => Ok(()),
            None => Err(self.abort(
                ExitCode::SysErrForbidden,
                format!("caller is not one of {}", self.message().from()),
            )),
        }
    }
    fn validate_immediate_caller_type<'a, I>(&self, types: I) -> Result<(), ActorError>
    where
        I: Iterator<Item = &'a Cid>,
    {
        let caller_cid = self.get_actor_code_cid(self.message().to())?;
        match types.filter(|c| **c == caller_cid).next() {
            Some(_) => Ok(()),
            None => Err(self.abort(
                ExitCode::SysErrForbidden,
                format!(
                    "caller cid type {} one of {}",
                    caller_cid,
                    self.message().from()
                ),
            )),
        }
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
        // TODO: Verify if right hash
        let c = self.store.put(obj, Blake2b256).map_err(|e| {
            self.abort(
                ExitCode::ErrPlaceholder,
                format!("storage put in create: {}", e.to_string()),
            )
        })?;
        // TODO: This is almost certainly wrong. Need to CBOR an empty slice and calculate CID
        self.state_commit(&Cid::default(), &c)
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
            })
            .and_then(|c| {
                c.ok_or_else(|| {
                    self.abort(
                        ExitCode::ErrPlaceholder,
                        "storage get error in  read only state".to_owned(),
                    )
                })
            })
    }

    fn transaction<C: Cbor, R, F>(&mut self, f: F) -> Result<R, ActorError>
    where
        F: FnOnce(&mut C, &BS) -> R,
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
            })
            .and_then(|c| {
                c.ok_or_else(|| {
                    self.abort(
                        ExitCode::ErrPlaceholder,
                        "storage get error in transaction".to_owned(),
                    )
                })
            })?;

        // Update the state
        let r = f(&mut state, &self.store);

        let c = self.store.put(&state, Blake2b256).map_err(|e| {
            self.abort(
                ExitCode::ErrPlaceholder,
                format!("storage put in create: {}", e.to_string()),
            )
        })?;

        // Committing that change
        self.state_commit(&act.state, &c)?;
        // return
        Ok(r)
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
        let msg = UnsignedMessage::builder()
            .to(to.clone())
            .from(self.message.from().clone())
            .method_num(method)
            .value(value.clone())
            .gas_limit(self.gas_available)
            .params(params.clone())
            .build()
            .unwrap();

        // snapshot state tree
        let snapshot = self
            .state
            .snapshot()
            .map_err(|_e| self.abort(ExitCode::ErrPlaceholder, "failed to create snapshot"))?;

        let mut parent = DefaultRuntime::new(
            self.state,
            self.store,
            self.gas_used,
            &msg,
            self.curr_epoch(),
            &self.origin,
            self.origin_nonce,
        );
        let send_res = internal_send::<ST, BS>(&mut parent, &msg, 0);
        self.state
            .revert_to_snapshot(&snapshot)
            .map_err(|_e| self.abort(ExitCode::ErrPlaceholder, "failed to revert snapshot"))?;
        send_res
    }

    fn abort<S: AsRef<str>>(&self, _exit_code: ExitCode, _msg: S) -> ActorError {
        todo!()
    }
    fn new_actor_address(&self) -> Address {
        todo!()
    }
    fn create_actor(&mut self, code_id: &Cid, address: &Address) -> Result<(), ActorError> {
        // TODO: Charge gas
        self.charge_gas(PLACEHOLDER_GAS);
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
        // TODO: Charge gas
        self.charge_gas(PLACEHOLDER_GAS);
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
}
pub fn internal_send<ST: StateTree, DB: BlockStore>(
    runtime: &mut DefaultRuntime<'_, '_, '_, ST, DB>,
    msg: &UnsignedMessage,
    _gas_cost: u64,
) -> Result<Serialized, ActorError> {
    // TODO: Calculate true gas value
    runtime.charge_gas(PLACEHOLDER_GAS);

    // TODO: we need to try to recover here and try to create account actor
    let to_actor = runtime
        .state
        .get_actor(msg.to())
        .map_err(|e| {
            ActorError::new(
                ExitCode::SysErrInternal,
                format!("failed to load actor in internal_send: {}", e),
            )
        })
        .and_then(|act| {
            act.ok_or_else(|| {
                ActorError::new(
                    ExitCode::SysErrInternal,
                    "actor not found in internal_send".to_owned(),
                )
            })
        })?;

    if msg.value() != &0u8.into() {
        transfer(runtime.state, &msg.from(), &msg.to(), &msg.value())
            .map_err(|e| ActorError::new(ExitCode::SysErrInternal, e))?;
    }

    let method_num = msg.method_num();

    if method_num != &METHOD_SEND {
        // TODO: charge gas

        let ret = {
            // TODO: make its own method/struct
            match to_actor.code {
                x if x == *SYSTEM_ACTOR_CODE_ID => {
                    actor::system::Actor.invoke_method(&mut *runtime, *method_num, msg.params())
                }
                x if x == *INIT_ACTOR_CODE_ID => {
                    actor::init::Actor.invoke_method(&mut *runtime, *method_num, msg.params())
                }
                x if x == *CRON_ACTOR_CODE_ID => {
                    actor::cron::Actor.invoke_method(&mut *runtime, *method_num, msg.params())
                }
                x if x == *ACCOUNT_ACTOR_CODE_ID => {
                    actor::account::Actor.invoke_method(&mut *runtime, *method_num, msg.params())
                }
                x if x == *POWER_ACTOR_CODE_ID => {
                    actor::power::Actor.invoke_method(&mut *runtime, *method_num, msg.params())
                }
                x if x == *MINER_ACTOR_CODE_ID => {
                    // not implemented yet
                    // actor::miner::Actor.invoke_method(&mut *runtime, *method_num, msg.params())
                    todo!()
                }
                x if x == *MARKET_ACTOR_CODE_ID => {
                    // not implemented yet
                    // actor::market::Actor.invoke_method(&mut *runtime, *method_num, msg.params())
                    todo!()
                }
                x if x == *PAYCH_ACTOR_CODE_ID => {
                    actor::paych::Actor.invoke_method(&mut *runtime, *method_num, msg.params())
                }
                x if x == *MULTISIG_ACTOR_CODE_ID => {
                    actor::cron::Actor.invoke_method(&mut *runtime, *method_num, msg.params())
                }
                x if x == *REWARD_ACTOR_CODE_ID => {
                    actor::cron::Actor.invoke_method(&mut *runtime, *method_num, msg.params())
                }
                _ => todo!("Handle unknown code cids"),
            }
        };
        return ret;
    }
    Ok(Serialized::default())
}

fn transfer<'a, ST>(
    state: &ST,
    from: &Address,
    to: &Address,
    value: &TokenAmount,
) -> Result<(), String>
where
    ST: StateTree,
{
    if from == to {
        return Ok(());
    }
    if value < &0u8.into() {
        return Err("Negative transfer value".to_owned());
    }
    let mut from_actor = state
        .get_actor(&from)
        .map_err(|e| format!("transfer failed: {}", e))?
        .ok_or_else(|| "transfer failed could not retrieve from actor".to_owned())?;
    let mut to_actor = state
        .get_actor(&to)
        .map_err(|e| format!("transfer failed: {}", e))?
        .ok_or_else(|| "transfer failed could not retrieve from actor".to_owned())?;

    deduct_funds(&mut from_actor, &value)?;
    deposit_funds(&mut to_actor, &value);
    Ok(())
}

fn deduct_funds(from: &mut ActorState, amt: &TokenAmount) -> Result<(), String> {
    if &from.balance < amt {
        return Err("not enough funds".to_owned());
    }
    from.balance -= amt;
    Ok(())
}
fn deposit_funds(to: &mut ActorState, amt: &TokenAmount) {
    to.balance += amt;
}
