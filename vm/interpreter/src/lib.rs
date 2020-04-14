// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use blocks::Tipset;
use clock::ChainEpoch;
use default_runtime::{internal_send, transfer, DefaultRuntime};
use forest_encoding::Cbor;
use ipld_blockstore::BlockStore;
use message::{Message, MessageReceipt, SignedMessage, UnsignedMessage};
use num_bigint::BigUint;
use num_traits::cast::ToPrimitive;
use num_traits::Zero;
use vm::ActorError;
use vm::{ExitCode, Serialized, StateTree};

/// Interpreter which handles execution of state transitioning messages and returns receipts
/// from the vm execution.
pub struct VM<'a, ST: StateTree, DB: BlockStore> {
    state: ST,
    store: &'a DB,
    epoch: ChainEpoch,
    block_miner: Address,
    // TODO: missing fields
}

const GAS_PER_MESSAGE_BYTE: u64 = 2;

impl<'a, ST: StateTree, DB: BlockStore> VM<'a, ST, DB> {
    pub fn new(state: ST, store: &'a DB, epoch: ChainEpoch) -> Self {
        // TODO replace default block miner address
        VM {
            state,
            store,
            epoch,
            block_miner: Address::default(),
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
                receipts.push(self.apply_message(msg.message(), &block.miner)?);
            }
        }

        Ok(receipts)
    }

    /// Applies the state transition for a single message
    /// Returns receipts from the transaction.
    pub fn apply_message(
        &mut self,
        msg: &UnsignedMessage,
        _miner_addr: &Address,
    ) -> Result<MessageReceipt, String> {
        check_message(msg)?;

        let snapshot = self.state.snapshot()?;

        let mut from_act = self
            .state
            .get_actor(msg.from())?
            .ok_or("Actor address could not be resolved")?;

        let ser_msg = &msg.marshal_cbor().map_err(|e| e.to_string())?;
        let msg_gas_cost = ser_msg.len() as u64 * GAS_PER_MESSAGE_BYTE;

        let gas_cost = msg.gas_price() * msg.gas_limit();
        let total_cost = &gas_cost + msg.value();
        if from_act.balance < total_cost {
            return Err(format!(
                "Not enough funds ({} < {})",
                total_cost, from_act.balance
            ));
        }

        transfer(&mut self.state, msg.from(), msg.to(), &gas_cost)?;

        if msg.sequence() != from_act.sequence {
            return Err(format!(
                "Invalid nonce (got: {}, expected: {})",
                msg.sequence(),
                from_act.sequence
            ));
        }
        from_act.sequence += 1;

        let (ret_data, mut gas_used, act_err) = self.send(msg, msg_gas_cost);

        if act_err.is_fatal() {
            return Err(format!("Fatal send actor error occurred, err: {}", act_err));
        };
        if act_err.exit_code() != ExitCode::Ok {
            gas_used = msg.gas_limit();
            // revert all state changes since snapshot
            if let Err(state_err) = self.state.revert_to_snapshot(&snapshot) {
                return Err(format!("Revert state failed: {}", state_err));
            };
        } else {
            // refund unused gas
            let refund = (msg.gas_limit().checked_sub(gas_used).unwrap_or(1u64)) * msg.gas_price();
            transfer(&mut self.state, msg.to(), msg.from(), &refund)?;
        };

        let miner = self
            .state
            .get_actor(&self.block_miner)?
            .ok_or("Actor address could not be resolved")?;

        // TODO: support multiple blocks in a tipset
        // TODO: actually wire this up (miner is undef for now)
        let gas_reward = msg.gas_price() * gas_used;
        transfer(&mut self.state, msg.to(), &self.block_miner, &gas_reward)?;
        if miner.balance != BigUint::zero() {
            return Err("Gas handling math is wrong".to_owned());
        }

        Ok(MessageReceipt {
            return_data: ret_data,
            exit_code: act_err.exit_code(),
            gas_used: BigUint::from(gas_used),
        })
    }
    /// Instantiates a new Runtime, and calls internal_send to do the execution.
    fn send(&mut self, msg: &UnsignedMessage, gas_cost: u64) -> (Serialized, u64, ActorError) {
        let mut rt = DefaultRuntime::new(
            &mut self.state,
            self.store,
            gas_cost,
            &msg,
            self.epoch,
            msg.from().clone(),
            msg.sequence(),
            0,
        );
        let ser = internal_send(&mut rt, msg, gas_cost)
            .map_err(|actor_err| (Serialized::default(), 0.to_u64().unwrap_or(1u64), actor_err));
        (
            ser.unwrap(),
            *rt.gas_used(),
            ActorError::new(ExitCode::Ok, "Ok actor error".to_owned()),
        )
    }
}

/// Does some basic checks on the Message to see if the fields are valid.
fn check_message(msg: &UnsignedMessage) -> Result<(), String> {
    if msg.gas_limit() == 0 {
        return Err("Message has no gas limit set".to_owned());
    }
    if msg.value() == &BigUint::zero() {
        return Err("Message has no value set".to_owned());
    }
    if msg.gas_price() == &BigUint::zero() {
        return Err("Message has no gas price set".to_owned());
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
