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
    chain: &'a ChainStore<DB>,
    // TODO: missing fiels
}

impl<'a, ST: StateTree, DB: BlockStore> VM<'a, ST, DB> {
    pub fn new(state: ST, chain: &'a ChainStore<DB>) -> Self {
        VM { state, chain }
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
        let snapshot = self.state.snapshot()?;
        let mut gas_cost = msg.gas_price() * msg.gas_limit();
        gas_cost += &msg.value().0;

        // TODO: gascost for message size

        // TODO: verify nonce of the from actor matches nonce of the message

        // TODO: check that the from actor has enough gas for the total gas cost

        // TODO: transfer gas

        // TODO: increase from actor nonce

        let return_data = self.send(None, msg, gas_cost.to_u64().unwrap());
        // where to get exit code?
        // match exit_code {
        //     ExitCode::Ok => {
        //         // all good
        //     }
        //     _ => {
        //         // TODO: handle fatal exit codes and return

        //         // Revert state on failed method execution
        //         self.state.revert_to_snapshot(&snapshot)?;
        //     }
        // }

        let receipt = MessageReceipt {
            return_data: return_data.unwrap(), // TODO: what about Send?
            exit_code: ExitCode::Ok,
            gas_used: BigUint::from(0u64), // TODO: get from runtime, runtime.gas_used()
        };

        Ok(receipt)
    }

    fn send(
        &mut self,
        parent_runtime: Option<&DefaultRuntime<ST, DB>>,
        msg: &UnsignedMessage,
        gas_cost: u64,
    ) -> Result<Serialized, ActorError> {
        // TODO: Those params should DEF not be default
        let mut rt = DefaultRuntime::new(
            &mut self.state,
            self.chain,
            gas_cost,
            msg,
            ChainEpoch::default(),
            Address::default(),
        );
        internal_send(RTType::New(&mut rt), msg, TokenAmount::new(gas_cost))
    }
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
    chain: &'a ChainStore<BS>,
    gas_used: u64,
    gas_available: u64,
    message: &'b UnsignedMessage,
    epoch: ChainEpoch,
    origin: Address,
}

impl<'a, 'b, 'c, ST: StateTree, BS: BlockStore> DefaultRuntime<'a, 'b, 'c, ST, BS> {
    pub fn new(
        state: &'c mut ST,
        chain: &'a ChainStore<BS>,
        gas_used: u64,
        message: &'b UnsignedMessage,
        epoch: ChainEpoch,
        origin: Address,
    ) -> Self {
        DefaultRuntime {
            state,
            chain,
            gas_used,
            gas_available: message.gas_limit(),
            message,
            epoch,
            origin,
        }
    }

    pub fn from_parent(
        state: &'c mut ST,
        chain: &'a ChainStore<BS>,
        gas_used: u64,
        message: &'b UnsignedMessage,
        epoch: ChainEpoch,
        origin: Address,
        parent: &DefaultRuntime<'a, 'b, 'c, ST, BS>,
    ) -> Self {
        DefaultRuntime {
            state,
            chain,
            gas_used: parent.gas_used + gas_used,
            gas_available: message.gas_limit(),
            message,
            epoch,
            origin,
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

        let mut x = addresses.filter(|a| **a == imm);
        match x.next() {
            Some(_) => Ok(()),
            None => Err(self.abort(
                ExitCode::SysErrForbidden,
                format!("caller is not one of {}", self.message().from()),
            )),
        }
    }
    fn validate_immediate_caller_type<'a, I>(&self, types: I)
    where
        I: Iterator<Item = &'a Cid>,
    {
        todo!()
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
    fn create<C: Cbor>(&self, obj: &C) {
        todo!()
    }
    fn state<C: Cbor>(&self) -> C {
        todo!()
    }
    fn transaction<C: Cbor, R, F>(&self, f: F) -> R
    where
        F: FnOnce(&mut C) -> R,
    {
        todo!()
    }

    fn store(&self) -> &BS {
        self.chain.blockstore()
    }

    fn send(
        &mut self,
        to: &Address,
        method: MethodNum,
        params: &Serialized,
        value: &TokenAmount,
    ) -> Result<Serialized, ActorError> {
        // TODO: snapshot and revert logic

        let msg = UnsignedMessage::builder()
            .to(to.clone())
            .from(self.message.to().clone())
            .method_num(method)
            .value(value.clone())
            .gas_limit(self.gas_available)
            .params(params.clone())
            .build()
            .unwrap(); // TODO: Handle error

        // let mut parent =  DefaultRuntime::from_parent(&mut self.state, &self.chain, TokenAmount::new(0), &msg, &self);

        // snapshot state tree
        let snapshot = self.state.snapshot().unwrap();

        let mut parent = DefaultRuntime::new(
            self.state,
            self.chain,
            0,
            &msg,
            self.curr_epoch(),
            self.origin.clone(),
        );
        // match internal_send::<ST, BS>(RTType::Parent(&mut parent), &msg, TokenAmount::new(0)) {
        //     Ok((code, res)) => {
        //         self.state.revert_to_snapshot(&snapshot).unwrap();
        //         Ok(res.unwrap())
        //     }
        //     Err(err) => {
        //         self.state.revert_to_snapshot(&snapshot).unwrap();
        //         panic!("{}", err);
        //     }
        // }
        todo!()
    }

    fn abort<S: AsRef<str>>(&self, exit_code: ExitCode, msg: S) -> ActorError {
        todo!()
    }
    fn new_actor_address(&self) -> Address {
        todo!()
    }
    fn create_actor(&mut self, code_id: &Cid, address: &Address) {
        // match self.state.set_actor(
        //     &address,
        //     ActorState::new(code_id.clone(), Cid::default(), 0u64.into(), 0),
        // ) {
        //     Err(_e) => self.abort(ExitCode::SysErrInternal, "creating actor entry".to_owned()),
        //     _ => {}
        // }
        todo!()
    }
    fn delete_actor(&mut self) {
        // TODO: Handle these unwraps
        let act = self.state.get_actor(self.message.to()).unwrap().unwrap();
        // .unwrap_or_else(|_e| {
        //     self.abort(
        //         ExitCode::SysErrInternal,
        //         "fail to load actor in delete actor".to_owned(),
        //     )
        // })
        // .unwrap_or_else(|| self.abort(
        //     ExitCode::SysErrInternal,
        //     "fail to load actor in delete actor".to_owned(),
        // ));

        // if act.balance != 0u8.into() {
        //     self.abort(
        //         ExitCode::SysErrInternal,
        //         "cannot delete actor with non-zero balance".to_owned(),
        //     )
        // }
        // if self.state.delete_actor(self.message.to()).is_err() {
        //     self.abort(
        //         ExitCode::SysErrInternal,
        //         "failed to delete actor".to_owned(),
        //     )
        // }
        todo!()
    }
}
enum RTType<'a, ST: StateTree, DB: BlockStore> {
    New(&'a mut DefaultRuntime<'a, 'a, 'a, ST, DB>),
    Parent(&'a mut DefaultRuntime<'a, 'a, 'a, ST, DB>),
}

fn internal_send<SR: Cbor, ST: StateTree, DB: BlockStore>(
    // state: &mut ST, // delete this
    // chain: &ChainStore<DB>,
    parent_runtime: RTType<'_, ST, DB>, // this mutable ref
    msg: &UnsignedMessage,
    gas_cost: TokenAmount,
) -> Result<SR, ActorError> {
    let mut runtime: &mut DefaultRuntime<ST, DB> = match parent_runtime {
        New(e) => e,
        Parent(e) => e,
    };

    //////// - rt internal send - ////////

    // do that snapshotting outside
    // let snapshot = runtime.state.snapshot()?;

    // Charge for method invocation
    runtime.charge_gas(PLACEHOLDER_NUMBER);
    // do the vm send stuff here

    let from_actor = runtime.state.get_actor(msg.from()).unwrap().unwrap();
    // TODO: i think we should try to recover here and try to create account actor
    let to_actor = runtime.state.get_actor(msg.to()).unwrap().unwrap();

    let get_charge = 0u64;
    let gas_used = 0u64;

    let origin = msg.from();
    let on = msg.sequence();

    let nac: u64 = 0;

    // TODO: This does some check to see if parent is null. since this is from rt internal send,
    // the parent is null. But we make it outside of this function.
    // still need to set parent.gasused to rt.gasused

    // end do the vm send stuff

    // defer
    // did the snapshotting outside
    // runtime.state.revert_to_snapshot(&snapshot)?;
    ////// end rt internal send

    // TODO: if to_actor doesn't exist try to create it
    runtime.charge_gas(PLACEHOLDER_NUMBER);

    let method_num = msg.method_num();

    if method_num != &MethodNum::new(METHOD_SEND as u64) {
        // TODO: charge gas

        let ret = {
            // TODO: make its own method/struct
            match to_actor {
                SYSTEM_ACTOR_CODE_ID => {
                    todo!("system actor");
                }
                INIT_ACTOR_CODE_ID => {
                    let actor = actor::init::Actor;
                    actor.invoke_method(&mut *runtime, *method_num, msg.params())
                }
                CRON_ACTOR_CODE_ID => {
                    let actor = actor::cron::Actor;
                    actor.invoke_method(&mut *runtime, *method_num, msg.params())
                }
                ACCOUNT_ACTOR_CODE_ID => todo!(),
                POWER_ACTOR_CODE_ID => todo!(),
                MINER_ACTOR_CODE_ID => todo!(),
                MARKET_ACTOR_CODE_ID => todo!(),
                PAYCH_ACTOR_CODE_ID => todo!(),
                MULTISIG_ACTOR_CODE_ID => todo!(),
                REWARD_ACTOR_CODE_ID => todo!(),
                _ => todo!("Handle unknown code cids"),
            }
        };
        let exit_code = ExitCode::Ok; // TODO: get from invocation
                                      // return ret;
    }
    todo!()
    // Ok((ExitCode::Ok, None))
}
