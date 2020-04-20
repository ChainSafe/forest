// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::{ACCOUNT_ACTOR_CODE_ID, REWARD_ACTOR_ADDR};
use address::Address;
use blocks::Tipset;
use clock::ChainEpoch;
use default_runtime::{internal_send, DefaultRuntime};
use forest_encoding::Cbor;
use ipld_blockstore::BlockStore;
use log::warn;
use message::{Message, MessageReceipt, SignedMessage, UnsignedMessage};
use num_bigint::BigUint;
use num_traits::Zero;
use runtime::Syscalls;
use vm::{price_list_by_epoch, ActorError, ExitCode, Serialized, StateTree};

/// Interpreter which handles execution of state transitioning messages and returns receipts
/// from the vm execution.
pub struct VM<'a, ST, DB, SYS> {
    state: ST,
    store: &'a DB,
    epoch: ChainEpoch,
    syscalls: SYS,
    // TODO: missing fields
}

impl<'a, ST, DB, SYS> VM<'a, ST, DB, SYS>
where
    ST: StateTree,
    DB: BlockStore,
    SYS: Syscalls + Copy,
{
    pub fn new(state: ST, store: &'a DB, epoch: ChainEpoch, syscalls: SYS) -> Self {
        VM {
            state,
            store,
            epoch,
            syscalls,
        }
    }

    /// Returns ChainEpoch
    fn epoch(&self) -> ChainEpoch {
        self.epoch
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
                receipts.push(self.apply_message(msg)?);
            }

            for msg in &block.secp_messages {
                receipts.push(self.apply_message(msg.message())?);
            }
        }

        Ok(receipts)
    }

    fn _apply_implicit_message(&mut self, msg: &UnsignedMessage) -> ApplyRet {
        let (ret_data, _, act_err) = self.send(msg, 0);

        if let Some(err) = act_err {
            return ApplyRet::new(
                MessageReceipt {
                    return_data: ret_data,
                    exit_code: err.exit_code(),
                    gas_used: BigUint::zero(),
                },
                BigUint::zero(),
                Some(err),
            );
        };

        ApplyRet::new(
            MessageReceipt {
                return_data: ret_data,
                exit_code: ExitCode::Ok,
                gas_used: BigUint::zero(),
            },
            BigUint::zero(),
            Some(ActorError::new(ExitCode::Ok, "Ok error".to_owned())),
        )
    }

    /// Applies the state transition for a single message
    /// Returns ApplyRet structure which contains the message receipt and some meta data.
    fn apply_message(&mut self, msg: &UnsignedMessage) -> Result<ApplyRet, String> {
        check_message(msg)?;

        let pl = price_list_by_epoch(self.epoch());
        let ser_msg = &msg.marshal_cbor().map_err(|e| e.to_string())?;
        let msg_gas_cost = pl.on_chain_message(ser_msg.len() as i64) as u64;

        if msg_gas_cost > msg.gas_limit() {
            return Ok(ApplyRet::new(
                MessageReceipt {
                    return_data: Serialized::default(),
                    exit_code: ExitCode::SysErrOutOfGas,
                    gas_used: BigUint::zero(),
                },
                msg.gas_price() * msg_gas_cost,
                Some(ActorError::new(
                    ExitCode::SysErrOutOfGas,
                    "Out of gas".to_owned(),
                )),
            ));
        }

        let miner_penalty_amount = msg.gas_price() * msg_gas_cost;
        let mut from_act = match self.state.get_actor(msg.from()) {
            Ok(from_act) => from_act.ok_or("Failed to retrieve actor state")?,
            Err(_) => {
                return Ok(ApplyRet::new(
                    MessageReceipt {
                        return_data: Serialized::default(),
                        exit_code: ExitCode::SysErrSenderInvalid,
                        gas_used: BigUint::zero(),
                    },
                    msg.gas_price() * msg_gas_cost,
                    Some(ActorError::new(
                        ExitCode::SysErrSenderInvalid,
                        "Sender invalid".to_owned(),
                    )),
                ));
            }
        };

        if from_act.code != *ACCOUNT_ACTOR_CODE_ID {
            return Ok(ApplyRet::new(
                MessageReceipt {
                    return_data: Serialized::default(),
                    exit_code: ExitCode::SysErrSenderInvalid,
                    gas_used: BigUint::zero(),
                },
                miner_penalty_amount,
                Some(ActorError::new(
                    ExitCode::SysErrSenderInvalid,
                    "Sender invalid".to_owned(),
                )),
            ));
        };

        if msg.sequence() != from_act.sequence {
            return Ok(ApplyRet::new(
                MessageReceipt {
                    return_data: Serialized::default(),
                    exit_code: ExitCode::SysErrSenderStateInvalid,
                    gas_used: BigUint::zero(),
                },
                miner_penalty_amount,
                Some(ActorError::new(
                    ExitCode::SysErrSenderStateInvalid,
                    "Sender state invalid".to_owned(),
                )),
            ));
        };

        let gas_cost = msg.gas_price() * msg.gas_limit();
        // TODO requires network_tx_fee to be added as per the spec
        let total_cost = &gas_cost + msg.value();
        if from_act.balance < total_cost {
            return Ok(ApplyRet::new(
                MessageReceipt {
                    return_data: Serialized::default(),
                    exit_code: ExitCode::SysErrSenderStateInvalid,
                    gas_used: BigUint::zero(),
                },
                miner_penalty_amount,
                Some(ActorError::new(
                    ExitCode::SysErrSenderStateInvalid,
                    "Sender state invalid".to_owned(),
                )),
            ));
        };

        self.state.mutate_actor(msg.from(), |act| {
            from_act.deduct_funds(&gas_cost)?;
            act.sequence += 1;
            Ok(())
        })?;

        let snapshot = self.state.snapshot()?;

        // scoped to deal with mutable reference borrowing
        let (ret_data, gas_used, act_err) = {
            let (ret_data, mut rt, act_err) = self.send(msg, msg_gas_cost as i64);
            rt.charge_gas(rt.price_list().on_chain_return_value(ret_data.len()))
                .map_err(|e| e.to_string())?;
            (ret_data, rt.gas_used(), act_err)
        };

        if let Some(err) = act_err {
            if err.is_fatal() {
                return Err(format!("Fatal send actor error occurred, err: {:?}", err));
            };
            if err.exit_code() != ExitCode::Ok {
                // revert all state changes since snapshot
                if let Err(state_err) = self.state.revert_to_snapshot(&snapshot) {
                    return Err(format!("Revert state failed: {}", state_err));
                };
            }
            warn!("Send actor error: from:{}, to:{}", msg.from(), msg.to());
        }
        let gas_used = if gas_used < 0 { 0 } else { gas_used as u64 };
        // refund unused gas
        let refund = (msg.gas_limit() - gas_used) * msg.gas_price();
        self.state.mutate_actor(msg.from(), |act| {
            act.deposit_funds(&refund);
            Ok(())
        })?;

        let gas_reward = msg.gas_price() * BigUint::from(gas_used);
        self.state.mutate_actor(&*REWARD_ACTOR_ADDR, |act| {
            act.deposit_funds(&gas_reward);
            Ok(())
        })?;

        if refund + gas_reward != gas_cost {
            return Err("Gas handling math is wrong".to_owned());
        }

        Ok(ApplyRet::new(
            MessageReceipt {
                return_data: ret_data,
                exit_code: ExitCode::Ok,
                gas_used: BigUint::from(gas_used),
            },
            BigUint::zero(),
            None,
        ))
    }
    /// Instantiates a new Runtime, and calls internal_send to do the execution.
    fn send<'m>(
        &mut self,
        msg: &'m UnsignedMessage,
        gas_cost: i64,
    ) -> (
        Serialized,
        DefaultRuntime<'_, 'm, '_, ST, DB, SYS>,
        Option<ActorError>,
    ) {
        let mut rt = DefaultRuntime::new(
            &mut self.state,
            self.store,
            self.syscalls,
            gas_cost,
            &msg,
            self.epoch,
            msg.from().clone(),
            msg.sequence(),
            0,
        );

        let ser = match internal_send(&mut rt, msg, gas_cost) {
            Ok(ser) => ser,
            Err(actor_err) => return (Serialized::default(), rt, Some(actor_err)),
        };
        (ser, rt, None)
    }
}

// TODO remove allow dead_code
#[allow(dead_code)]
/// Apply message return data
pub struct ApplyRet {
    msg_receipt: MessageReceipt,
    penalty: BigUint,
    act_error: Option<ActorError>,
}

impl ApplyRet {
    fn new(msg_receipt: MessageReceipt, penalty: BigUint, act_error: Option<ActorError>) -> Self {
        Self {
            msg_receipt,
            penalty,
            act_error,
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
    _miner: Address,      // The block miner's actor address
    _post_proof: Vec<u8>, // The miner's Election PoSt proof output
}

/// Represents the messages from a tipset, grouped by block.
pub struct TipSetMessages {
    blocks: Vec<BlockMessages>,
    _epoch: ChainEpoch,
}
