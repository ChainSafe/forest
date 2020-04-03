// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use blocks::Tipset;
use clock::ChainEpoch;
use default_runtime::{internal_send, DefaultRuntime};
use ipld_blockstore::BlockStore;
use message::{Message, MessageReceipt, SignedMessage, UnsignedMessage};
use num_bigint::BigUint;
use num_traits::cast::ToPrimitive;
use vm::ActorError;
use vm::{ExitCode, Serialized, StateTree};

/// Interpreter which handles execution of state transitioning messages and returns receipts
/// from the vm execution.
pub struct VM<'a, ST: StateTree, DB: BlockStore> {
    state: ST,
    store: &'a DB,
    epoch: ChainEpoch,
    // TODO: missing fields
}

impl<'a, ST: StateTree, DB: BlockStore> VM<'a, ST, DB> {
    pub fn new(state: ST, store: &'a DB, epoch: ChainEpoch) -> Self {
        VM {
            state,
            store,
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
                if let Err(state_err) = self.state.revert_to_snapshot(&snapshot) {
                    return state_err;
                }
                e.to_string()
            })?
    }
    /// Instantiates a new Runtime, and calls internal_send to do the execution.
    fn send(&mut self, msg: &UnsignedMessage, gas_cost: u64) -> Result<Serialized, ActorError> {
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
        internal_send(&mut rt, msg, gas_cost)
    }
}

/// Does some basic checks on the Message to see if the fields are valid.
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
