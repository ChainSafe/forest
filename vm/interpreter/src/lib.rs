// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::{ActorState, ACCOUNT_ACTOR_CODE_ID, REWARD_ACTOR_ADDR};
use address::Address;
use blocks::Tipset;
use clock::ChainEpoch;
use default_runtime::{
    internal_send, transfer_from_gas_holder, transfer_to_gas_holder, DefaultRuntime,
};
use forest_encoding::Cbor;
use ipld_blockstore::BlockStore;
use message::{Message, MessageReceipt, SignedMessage, UnsignedMessage};
use num_bigint::BigUint;
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
    ) -> Result<Vec<ApplyRet>, String> {
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
    ) -> Result<ApplyRet, String> {
        check_message(msg)?;

        let ser_msg = &msg.marshal_cbor().map_err(|e| e.to_string())?;
        let msg_gas_cost = ser_msg.len() as u64 * GAS_PER_MESSAGE_BYTE;
        if msg_gas_cost > msg.gas_limit() {
            // TODO add duration
            return Ok(ApplyRet {
                msg_receipt: MessageReceipt {
                    return_data: Serialized::default(),
                    exit_code: ExitCode::SysErrOutOfGas,
                    gas_used: BigUint::zero(),
                },
                penalty: msg.gas_price() * msg_gas_cost,
                act_err: ActorError::new(ExitCode::SysErrOutOfGas, "Out of gas".to_owned()),
            });
        }

        let miner_penalty_amount = msg.gas_price() * msg_gas_cost;
        let mut from_act = match self.state.get_actor(msg.from()) {
            Ok(from_act) => from_act.ok_or("Failed to retrieve actor state")?,
            Err(_) => {
                return Ok(ApplyRet {
                    msg_receipt: MessageReceipt {
                        return_data: Serialized::default(),
                        exit_code: ExitCode::SysErrSenderInvalid,
                        gas_used: BigUint::zero(),
                    },
                    penalty: msg.gas_price() * msg_gas_cost,
                    act_err: ActorError::new(
                        ExitCode::SysErrSenderInvalid,
                        "Sender invalid".to_owned(),
                    ),
                });
            }
        };

        if from_act.code != *ACCOUNT_ACTOR_CODE_ID {
            return Ok(ApplyRet {
                msg_receipt: MessageReceipt {
                    return_data: Serialized::default(),
                    exit_code: ExitCode::SysErrSenderInvalid,
                    gas_used: BigUint::zero(),
                },
                penalty: miner_penalty_amount,
                act_err: ActorError::new(
                    ExitCode::SysErrSenderInvalid,
                    "Sender invalid".to_owned(),
                ),
            });
        };

        if msg.sequence() != from_act.sequence {
            return Ok(ApplyRet {
                msg_receipt: MessageReceipt {
                    return_data: Serialized::default(),
                    exit_code: ExitCode::SysErrSenderStateInvalid,
                    gas_used: BigUint::zero(),
                },
                penalty: miner_penalty_amount,
                act_err: ActorError::new(
                    ExitCode::SysErrSenderStateInvalid,
                    "Sender state invalid".to_owned(),
                ),
            });
        };

        let gas_cost = msg.gas_price() * msg.gas_limit();
        let total_cost = &gas_cost + msg.value(); // TODO requires network_tx_fee to be added
        if from_act.balance < total_cost {
            return Ok(ApplyRet {
                msg_receipt: MessageReceipt {
                    return_data: Serialized::default(),
                    exit_code: ExitCode::SysErrSenderStateInvalid,
                    gas_used: BigUint::zero(),
                },
                penalty: miner_penalty_amount,
                act_err: ActorError::new(
                    ExitCode::SysErrSenderStateInvalid,
                    "Sender state invalid".to_owned(),
                ),
            });
        };

        let mut gas_holder = ActorState::default();
        transfer_to_gas_holder(&mut self.state, msg.from(), &mut gas_holder, &gas_cost)?;
        from_act.sequence += 1;

        let snapshot = self.state.snapshot()?;

        let (ret_data, act_err, gas_used) = {
            let (ret_data, rt, act_err) = self.send(msg, msg_gas_cost);
            if act_err.is_fatal() {
                return Err(format!("Fatal send actor error occurred, err: {}", act_err));
            };
            // charge gas
            // rt.charge_gas(..)..
            (ret_data, act_err, rt.gas_used())
        };

        // TODO rt.chargeGasSafe

        if act_err.exit_code() != ExitCode::Ok {
            // revert all state changes since snapshot
            if let Err(state_err) = self.state.revert_to_snapshot(&snapshot) {
                return Err(format!("Revert state failed: {}", state_err));
            };
        }
        // TODO free tx
        // refund unused gas
        let refund =
            (msg.gas_limit().checked_sub(*rt.gas_used()).unwrap_or(1u64)) * msg.gas_price();
        transfer_from_gas_holder(&mut self.state, msg.from(), &mut gas_holder, &refund)?;

        let gas_reward = msg.gas_price() * rt.gas_used();
        transfer_from_gas_holder(
            &mut self.state,
            &*REWARD_ACTOR_ADDR,
            &mut gas_holder,
            &gas_reward,
        )?;

        if gas_holder.balance != BigUint::zero() {
            return Err("Gas handling math is wrong".to_owned());
        }

        Ok(ApplyRet {
            msg_receipt: MessageReceipt {
                return_data: ret_data,
                exit_code: act_err.exit_code(),
                gas_used: BigUint::from(*rt.gas_used()),
            },
            penalty: BigUint::zero(),
            act_err,
        })
    }
    /// Instantiates a new Runtime, and calls internal_send to do the execution.
    fn send(
        &mut self,
        msg: &UnsignedMessage,
        gas_cost: u64,
    ) -> (Serialized, DefaultRuntime<ST, DB>, ActorError) {
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

        // TRY TO AVOID PASSING BACK RUNTIME
        let ser = match internal_send(&mut rt, msg, gas_cost) {
            Ok(ser) => ser,
            Err(actor_err) => return (Serialized::default(), rt, actor_err),
        };
        // charge gas
        (
            ser,
            // Austin:
            rt.gas_used,
            ActorError::new(ExitCode::Ok, "Ok actor error".to_owned()),
        )
    }
}

struct ApplyRet {
    msg_receipt: MessageReceipt,
    act_err: ActorError,
    penalty: BigUint,
}

impl ApplyRet {
    fn new(msg_receipt: MessageReceipt, act_err: ActorError, penalty: BigUint) -> Self {
        Self {
            msg_receipt,
            act_err,
            penalty,
        }
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
