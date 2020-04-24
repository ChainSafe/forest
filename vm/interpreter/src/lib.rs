// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::{
    cron, reward, ACCOUNT_ACTOR_CODE_ID, CRON_ACTOR_ADDR, REWARD_ACTOR_ADDR, SYSTEM_ACTOR_ADDR,
};
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
use std::collections::HashSet;
use std::error::Error as StdError;
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
    ) -> Result<Vec<MessageReceipt>, Box<dyn StdError>> {
        let mut receipts = Vec::new();
        let mut processed = HashSet::<Cid>::default();

        for block in tipset.blocks() {
            let mut penalty = BigUint::zero();
            let mut gas_reward = BigUint::zero();

            let mut process_msg = |msg: &UnsignedMessage| -> Result<(), Box<dyn StdError>> {
                let cid = msg.cid()?;
                // Ensure no duplicate processing of a message
                if processed.contains(&cid) {
                    return Ok(());
                }
                let ret = self.apply_message(msg)?;

                // Update totals
                gas_reward += msg.gas_price() * ret.msg_receipt.gas_used;
                penalty += ret.penalty;
                receipts.push(ret.msg_receipt);

                // Add callback here if needed in future

                // Add processed Cid to set of processed messages
                processed.insert(cid);
                Ok(())
            };

            for msg in block.bls_msgs() {
                process_msg(msg)?;
            }
            for msg in block.secp_msgs() {
                process_msg(msg.message())?;
            }

            // Generate reward transaction for the miner of the block
            let params = Serialized::serialize(reward::AwardBlockRewardParams {
                miner: block.header().miner_address().clone(),
                penalty,
                gas_reward,
                // TODO revisit this if/when removed from go clients
                ticket_count: 1,
            })?;

            // TODO change this just just one get and update sequence in memory after interop
            let sys_act = self
                .state
                .get_actor(&*SYSTEM_ACTOR_ADDR)?
                .ok_or_else(|| "Failed to query system actor".to_string())?;

            let rew_msg = UnsignedMessage::builder()
                .from(SYSTEM_ACTOR_ADDR.clone())
                .to(REWARD_ACTOR_ADDR.clone())
                .sequence(sys_act.sequence)
                .value(BigUint::zero())
                .gas_price(BigUint::zero())
                .gas_limit(1 << 30)
                .method_num(reward::Method::AwardBlockReward as u64)
                .params(params)
                .build()?;

            // TODO revisit this ApplyRet structure, doesn't match go logic 1:1 and can be cleaner
            let ret = self.apply_implicit_message(&rew_msg);
            if let Some(err) = ret.act_error {
                return Err(format!(
                    "failed to apply reward message for miner {}: {}",
                    block.header().miner_address(),
                    err
                )
                .into());
            }

            // Add callback here for reward message if needed
        }

        // TODO same as above, unnecessary state retrieval
        let sys_act = self
            .state
            .get_actor(&*SYSTEM_ACTOR_ADDR)?
            .ok_or_else(|| "Failed to query system actor".to_string())?;

        let cron_msg = UnsignedMessage::builder()
            .from(SYSTEM_ACTOR_ADDR.clone())
            .to(CRON_ACTOR_ADDR.clone())
            .sequence(sys_act.sequence)
            .value(BigUint::zero())
            .gas_price(BigUint::zero())
            .gas_limit(1 << 30)
            .method_num(cron::Method::EpochTick as u64)
            .params(Serialized::default())
            .build()?;

        let ret = self.apply_implicit_message(&cron_msg);
        if let Some(err) = ret.act_error {
            return Err(format!("failed to apply block cron message: {}", err).into());
        }

        // Add callback here for cron message if needed
        Ok(receipts)
    }

    fn apply_implicit_message(&mut self, msg: &UnsignedMessage) -> ApplyRet {
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
            None,
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
                gas_used,
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
