// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use blocks::Tipset;
use chain::ChainStore;
use clock::ChainEpoch;
use ipld_blockstore::BlockStore;
use message::{Message, MessageReceipt, SignedMessage, UnsignedMessage};
use num_bigint::BigUint;
use runtime::{ActorCode, DefaultRuntime, Runtime};
use vm::{ExitCode, MethodNum, Serialized, StateTree, TokenAmount, METHOD_SEND};

/// Interpreter which handles execution of state transitioning messages and returns receipts
/// from the vm execution.
pub struct VM<ST: StateTree> {
    state: ST,
} // TODO add context necessary

impl<ST: StateTree> VM<ST> {
    pub fn new(state: ST) -> Self {
        VM { state }
    }

    /// Apply all messages from a tipset
    /// Returns the receipts from the transactions.
    pub fn apply_tip_set_messages<DB: BlockStore>(
        &mut self,
        chain: &ChainStore<DB>,
        _tipset: &Tipset,
        msgs: &TipSetMessages,
    ) -> Result<Vec<MessageReceipt>, String> {
        let mut receipts = Vec::new();

        for block in &msgs.blocks {
            // TODO: verifiy ordering of message execution

            for msg in &block.bls_messages {
                receipts.push(self.apply_message(chain, msg, &block.miner)?);
            }

            for msg in &block.secp_messages {
                receipts.push(self.apply_message(chain, &msg.message(), &block.miner)?);
            }
        }

        Ok(receipts)
    }

    /// Applies the state transition for a single message
    /// Returns receipts from the transaction, and the miner penalty token amount.
    pub fn apply_message<BS: BlockStore>(
        &mut self,
        chain: &ChainStore<BS>,
        msg: &UnsignedMessage,
        _miner_addr: &Address,
    ) -> Result<MessageReceipt, String> {
        let snapshot = self.state.snapshot()?;
        let mut gas_cost: TokenAmount = (msg.gas_price() * msg.gas_limit()).into();
        gas_cost += msg.value().clone();

        // TODO: gascost for message size

        // TODO: verify nonce of the from actor matches nonce of the message

        // TODO: check that the from actor has enough gas for the total gas cost

        // TODO: transfer gas

        // TODO: increase from actor nonce

        let runtime = DefaultRuntime::new(chain);
        let (exit_code, return_data) = self.send(&runtime, msg, gas_cost)?;

        match exit_code {
            ExitCode::Ok => {
                // all good
            }
            _ => {
                // TODO: handle fatal exit codes and return

                // Revert state on failed method execution
                self.state.revert_to_snapshot(&snapshot)?;
            }
        }

        let receipt = MessageReceipt {
            return_data: return_data.unwrap(), // TODO: what about Send?
            exit_code,
            gas_used: BigUint::from(0u64), // TODO: get from runtime, runtime.gas_used()
        };

        Ok(receipt)
    }

    fn send<BS: BlockStore>(
        &mut self,
        runtime: &DefaultRuntime<BS>,
        msg: &UnsignedMessage,
        gas_cost: TokenAmount,
    ) -> Result<(ExitCode, Option<Serialized>), String> {
        let from_actor = self.state.get_actor(msg.from())?;

        let to_actor = self.state.get_actor(msg.to())?;
        // TODO: if to_actor doesn't exist try to create it

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
                        actor.invoke_method(&runtime, *method_num, msg.params())
                    }
                    CRON_ACTOR_CODE_ID => todo!(),
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
            return Ok((exit_code, Some(ret)));
        }

        Ok((ExitCode::Ok, None))
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
