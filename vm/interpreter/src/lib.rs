// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::{ACCOUNT_ACTOR_CODE_ID, REWARD_ACTOR_ADDR};
use blocks::FullTipset;
use cid::Cid;
use clock::ChainEpoch;
use default_runtime::{internal_send, DefaultRuntime};
use forest_encoding::Cbor;
use ipld_blockstore::BlockStore;
use log::warn;
use message::{Message, MessageReceipt, UnsignedMessage};
use num_bigint::BigUint;
use num_traits::Zero;
use runtime::Syscalls;
use state_tree::StateTree;
use vm::{price_list_by_epoch, ActorError, ExitCode, Serialized};

/// Interpreter which handles execution of state transitioning messages and returns receipts
/// from the vm execution.
pub struct VM<'db, DB, SYS> {
    state: StateTree<'db, DB>,
    // TODO revisit handling buffered store specifically in VM
    store: &'db DB,
    epoch: ChainEpoch,
    syscalls: SYS,
    // TODO: missing fields
}

impl<'db, DB, SYS> VM<'db, DB, SYS>
where
    DB: BlockStore,
    SYS: Syscalls + Copy,
{
    pub fn new(
        root: &Cid,
        store: &'db DB,
        epoch: ChainEpoch,
        syscalls: SYS,
    ) -> Result<Self, String> {
        let state = StateTree::new_from_root(store, root)?;
        Ok(VM {
            state,
            store,
            epoch,
            syscalls,
        })
    }

    /// Flush stores in VM and return state root.
    pub fn flush(&mut self) -> Result<Cid, String> {
        self.state.flush()
    }

    /// Returns ChainEpoch
    fn epoch(&self) -> ChainEpoch {
        self.epoch
    }

    /// Apply all messages from a tipset
    /// Returns the receipts from the transactions.
    pub fn apply_tip_set_messages(
        &mut self,
        tipset: &FullTipset,
    ) -> Result<Vec<MessageReceipt>, String> {
        let mut receipts = Vec::new();

        for block in tipset.blocks() {
            for msg in block.bls_msgs() {
                receipts.push(self.apply_message(msg)?.msg_receipt);
            }

            for msg in block.secp_msgs() {
                receipts.push(self.apply_message(msg.message())?.msg_receipt);
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
                    gas_used: 0,
                },
                BigUint::zero(),
                Some(err),
            );
        };

        ApplyRet::new(
            MessageReceipt {
                return_data: ret_data,
                exit_code: ExitCode::Ok,
                gas_used: 0,
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
                    gas_used: 0,
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
                        gas_used: 0,
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
                    gas_used: 0,
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
                    gas_used: 0,
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
                    gas_used: 0,
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
                gas_used: gas_used,
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
        DefaultRuntime<'db, 'm, '_, DB, SYS>,
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
