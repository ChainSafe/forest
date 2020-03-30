// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::RTType::New;
use crate::RTType::Parent;
use address::Address;
use blocks::Tipset;
use chain::ChainStore;
use cid::Cid;
use clock::ChainEpoch;
use crypto::DomainSeparationTag;
use forest_encoding::Cbor;
use ipld_blockstore::BlockStore;
use message::{Message, MessageReceipt, SignedMessage, UnsignedMessage};
use num_bigint::BigUint;
use num_traits::cast::ToPrimitive;
use runtime::{ActorCode, Runtime};
use vm::ActorError;
use vm::{
    ActorState, ExitCode, MethodNum, Randomness, Serialized, StateTree, TokenAmount, METHOD_SEND,
};

const PLACEHOLDER_NUMBER: u64 = 1;
/// Interpreter which handles execution of state transitioning messages and returns receipts
/// from the vm execution.
pub struct VM<'a, ST: StateTree, DB: BlockStore> {
    state: ST,
    chain: &'a DB,
    epoch: ChainEpoch,
    // TODO: missing fields
}

impl<'a, ST: StateTree, DB: BlockStore> VM<'a, ST, DB> {
    pub fn new(
        state: ST,
        chain: &'a DB,
        epoch: ChainEpoch,
    ) -> Self {
        VM {
            state,
            chain,
            epoch,
        }
    }

    /// Apply all messages from a tipset
    /// Returns the receipts from the transactions.
    pub fn apply_tip_set_messages(
        &mut self,
        _tipset: &Tipset,
        msgs: &TipSetMessages,
    ) -> Result<Vec<MessageReceipt>, String> {
        let mut receipts = Vec::new();

        for block in &msgs.blocks {
            // TODO: verifiy ordering of message execution

            for msg in &block.bls_messages {
                receipts.push(self.apply_message(msg, &block.miner)?);
            }

            for msg in &block.secp_messages {
                receipts.push(self.apply_message(&msg.message(), &block.miner)?);
            }
        }

        Ok(receipts)
    }

    /// Applies the state transition for a single message
    /// Returns receipts from the transaction, and the miner penalty token amount.
    pub fn apply_message(
        &mut self,
        msg: &UnsignedMessage,
        _miner_addr: &Address,
    ) -> Result<MessageReceipt, String> {
        check_message(&msg)?;

        let snapshot = self.state.snapshot()?;

        // TODO: Not the complete gas_cost. Calculate based on message size
        let mut gas_cost = msg.gas_price() * msg.gas_limit();
        gas_cost += msg.value();

        // TODO: gascost for message size

        // TODO: verify nonce of the from actor matches nonce of the message

        // TODO: check that the from actor has enough gas for the total gas cost

        // TODO: transfer gas

        // TODO: increase from actor nonce

        let return_data = self.send(msg, gas_cost.to_u64().unwrap());
        return_data
            .map(|r| {
                Ok(MessageReceipt {
                    return_data: r, // TODO: what about Send?,
                    exit_code: ExitCode::Ok,
                    gas_used: BigUint::from(0u64), // TODO: get from runtime, runtime.gas_used()
                })
            })
            .map_err(|e| {
                match self.state.revert_to_snapshot(&snapshot) {
                    Err(state_err) => return state_err,
                    _ => {}
                };
                e.to_string()
            })?
    }

    fn send(
        &mut self,
        msg: &UnsignedMessage,
        gas_cost: u64,
    ) -> Result<Serialized, ActorError> {
        // TODO: Those params should DEF not be default
        let mut rt = DefaultRuntime::new(
            &mut self.state,
            self.chain,
            gas_cost,
            &msg,
            self.epoch,
            &msg.from(),
            msg.sequence(),
        );
        internal_send(RTType::New(&mut rt), msg, gas_cost)
    }
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
        .ok_or(format!("transfer failed could not retrieve from actor"))?;
    let mut to_actor = state
        .get_actor(&to)
        .map_err(|e| format!("transfer failed: {}", e))?
        .ok_or(format!("transfer failed could not retrieve from actor"))?;

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
fn check_message(msg: &UnsignedMessage) -> Result<(), String> {
    if msg.gas_limit() == 0 {
        return Err("Gas limit is 0".to_owned());
    }
    Ok(())
}
/// Represents the messages from one block in a tipset.
pub struct BlockMessages {
    bls_messages: Vec<UnsignedMessage>,
    secp_messages: Vec<SignedMessage>,
    miner: Address,       // The block miner's actor address
    _post_proof: Vec<u8>, // The miner's Election PoSt proof output
}

/// Represents the messages from a tipset, grouped by block.
pub struct TipSetMessages {
    blocks: Vec<BlockMessages>,
    _epoch: ChainEpoch,
}

pub struct DefaultRuntime<'a, 'b, 'c, ST: StateTree, BS: BlockStore> {
    state: &'c mut ST,
    chain: &'a BS,
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
        chain: &'a BS,
        gas_used: u64,
        message: &'b UnsignedMessage,
        epoch: ChainEpoch,
        origin: &Address,
        origin_nonce: u64,
    ) -> Self {
        DefaultRuntime {
            state,
            chain,
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

    pub fn get_balance(&self, addr: &Address) -> Result<BigUint, ExitCode> {
        let act = self.state.get_actor(&addr).unwrap().unwrap();
        Ok(act.balance)
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
        // TODO: Specs actor calls this "Caller". Need to verify whats right
        let imm = self.resolve_address(self.message().from()).unwrap();

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
            .map(|bal| bal.into())
            .map_err(|e| self.abort(e, "Error getting current balance"))
    }
    fn resolve_address(&self, address: &Address) -> Result<Address, ActorError> {
        self.state
            .lookup_id(&address)
            .map_err(|e| self.abort(ExitCode::ErrPlaceholder, e))
    }
    fn get_actor_code_cid(&self, addr: &Address) -> Result<Cid, ActorError> {
        self.state
            .get_actor(&addr)
            .map(|act| act.unwrap().code)
            .map_err(|e| self.abort(ExitCode::ErrPlaceholder, e))
    }
    fn get_randomness(
        personalization: DomainSeparationTag,
        rand_epoch: ChainEpoch,
        entropy: &[u8],
    ) -> Randomness {
        todo!()
    }
    // fn create<C: Cbor>(&mut self, obj: &C) {
    //     todo!()
    // }
    fn create<C: Cbor>(&self, obj: &C) {
        todo!()
    }
    // readonly
    fn state<C: Cbor>(&self) -> C {
        todo!()
    }
    fn transaction<C: Cbor, R, F>(&self, f: F) -> R
    where
        F: FnOnce(&mut C) -> R,
    {
        todo!()
    }
    // fn transaction<C: Cbor, R, F>(&mut self, f: F) -> R
    // where
    //     F: FnOnce(&mut C, &BS) -> R,
    // {
    //     // // get actor
    //     // let act = self.state.get_actor(&rt.message().to());

    //     // // get state for actor based on generic C
    //     // let state: C = self.bs.get(act.state);

    //     // // Update the state
    //     // let r = f(&mut state, &self.bs);

    //     // // Committing that change
    //     // // TODO commit state to blockstore with stateCommit eq

    //     // // return
    //     // r
    //     todo!()
    // }

    fn store(&self) -> &BS {
        self.chain
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
            .unwrap(); // TODO: Handle error

        // let mut parent =  DefaultRuntime::from_parent(&mut self.state, &self.chain, TokenAmount::new(0), &msg, &self);

        // snapshot state tree
        let snapshot = self.state.snapshot().map_err(|_e| {
            self.abort(ExitCode::ErrPlaceholder, "failed to create snapshot")
        })?;

        let mut parent = DefaultRuntime::new(
            self.state,
            self.chain,
            self.gas_used,
            &msg,
            self.curr_epoch(),
            &self.origin,
            self.origin_nonce,
        );
        let send_res = internal_send::<ST, BS>(RTType::Parent(&mut parent), &msg, 0);
        self.state.revert_to_snapshot(&snapshot).map_err(|_e| {
            self.abort(ExitCode::ErrPlaceholder, "failed to revert snapshot")
        })?;
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
        self.charge_gas(PLACEHOLDER_NUMBER);
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
        self.charge_gas(PLACEHOLDER_NUMBER);
        let balance = self
            .state
            .get_actor(self.message.to())
            .map_err(|e| {
                self.abort(
                    ExitCode::SysErrInternal,
                    format!("failed to load actore in delete actor: {}", e),
                )
            })
            .and_then(|act| {
                act.ok_or(self.abort(ExitCode::SysErrInternal, "actor not found in delete actor"))
            })
            .map(|act| act.balance)?;
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
enum RTType<'a, ST: StateTree, DB: BlockStore> {
    New(&'a mut DefaultRuntime<'a, 'a, 'a, ST, DB>),
    Parent(&'a mut DefaultRuntime<'a, 'a, 'a, ST, DB>),
}

fn internal_send<ST: StateTree, DB: BlockStore>(
    // state: &mut ST, // delete this
    // chain: &ChainStore<DB>,
    parent_runtime: RTType<'_, ST, DB>, // this mutable ref
    msg: &UnsignedMessage,
    _gas_cost: u64,
) -> Result<Serialized, ActorError> {
    let runtime: &mut DefaultRuntime<ST, DB> = match parent_runtime {
        New(e) => e,
        Parent(e) => e,
    };
    // TODO: Calculate true gas value
    runtime.charge_gas(PLACEHOLDER_NUMBER);

    // TODO: we need to try to recover here and try to create account actor
    let to_actor = runtime.state.get_actor(msg.to()).unwrap().unwrap();

    if msg.value() != &0u8.into() {
        transfer(runtime.state, &msg.from(), &msg.to(), &msg.value()).unwrap();
    }

    let method_num = msg.method_num();

    if method_num != &METHOD_SEND {
        // TODO: charge gas

        let ret = {
            // TODO: make its own method/struct
            match to_actor.code {
                x if x == *actor::SYSTEM_ACTOR_CODE_ID => {
                    actor::system::Actor.invoke_method(&mut *runtime, *method_num, msg.params())
                }
                x if x == *actor::INIT_ACTOR_CODE_ID => {
                    actor::init::Actor.invoke_method(&mut *runtime, *method_num, msg.params())
                }
                x if x == *actor::CRON_ACTOR_CODE_ID => {
                    actor::cron::Actor.invoke_method(&mut *runtime, *method_num, msg.params())
                }
                x if x == *actor::ACCOUNT_ACTOR_CODE_ID => {
                    actor::account::Actor.invoke_method(&mut *runtime, *method_num, msg.params())
                }
                x if x == *actor::POWER_ACTOR_CODE_ID => {
                    actor::power::Actor.invoke_method(&mut *runtime, *method_num, msg.params())
                }
                x if x == *actor::MINER_ACTOR_CODE_ID => {
                    // not implemented yet
                    // actor::miner::Actor.invoke_method(&mut *runtime, *method_num, msg.params())
                    todo!()
                }
                x if x == *actor::MARKET_ACTOR_CODE_ID => {
                    // not implemented yet
                    // actor::market::Actor.invoke_method(&mut *runtime, *method_num, msg.params())
                    todo!()
                }
                x if x == *actor::PAYCH_ACTOR_CODE_ID => {
                    actor::paych::Actor.invoke_method(&mut *runtime, *method_num, msg.params())
                }
                x if x == *actor::MULTISIG_ACTOR_CODE_ID => {
                    actor::cron::Actor.invoke_method(&mut *runtime, *method_num, msg.params())
                }
                x if x == *actor::REWARD_ACTOR_CODE_ID => {
                    actor::cron::Actor.invoke_method(&mut *runtime, *method_num, msg.params())
                }
                _ => todo!("Handle unknown code cids"),
            }
        };
        return ret;
    }
    Ok(Serialized::default())
}
