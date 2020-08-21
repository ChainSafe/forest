// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{gas_tracker::price_list_by_epoch, vm_send, ChainRand, DefaultRuntime};
use actor::{
    cron, reward, ACCOUNT_ACTOR_CODE_ID, BURNT_FUNDS_ACTOR_ADDR, CRON_ACTOR_ADDR,
    REWARD_ACTOR_ADDR, SYSTEM_ACTOR_ADDR,
};
use blocks::FullTipset;
use cid::Cid;
use clock::ChainEpoch;
use fil_types::NetworkParams;
use forest_encoding::Cbor;
use ipld_blockstore::BlockStore;
use log::warn;
use message::{Message, MessageReceipt, UnsignedMessage};
use num_bigint::BigInt;
use num_traits::Zero;
use runtime::Syscalls;
use state_tree::StateTree;
use std::collections::HashSet;
use std::convert::TryFrom;
use std::error::Error as StdError;
use std::marker::PhantomData;
use vm::{actor_error, ActorError, ExitCode, Serialized, TokenAmount};

const GAS_OVERUSE_NUM: i64 = 11;
const GAS_OVERUSE_DENOM: i64 = 10;

/// Interpreter which handles execution of state transitioning messages and returns receipts
/// from the vm execution.
pub struct VM<'db, 'r, DB, SYS, P> {
    state: StateTree<'db, DB>,
    store: &'db DB,
    epoch: ChainEpoch,
    syscalls: SYS,
    rand: &'r ChainRand,
    base_fee: BigInt,
    params: PhantomData<P>,
}

impl<'db, 'r, DB, SYS, P> VM<'db, 'r, DB, SYS, P>
where
    DB: BlockStore,
    SYS: Syscalls,
    P: NetworkParams,
{
    pub fn new(
        root: &Cid,
        store: &'db DB,
        epoch: ChainEpoch,
        syscalls: SYS,
        rand: &'r ChainRand,
        base_fee: BigInt,
    ) -> Result<Self, String> {
        let state = StateTree::new_from_root(store, root)?;
        Ok(VM {
            state,
            store,
            epoch,
            syscalls,
            rand,
            base_fee,
            params: PhantomData,
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

    pub fn state(&self) -> &StateTree<'_, DB> {
        &self.state
    }

    /// Apply all messages from a tipset
    /// Returns the receipts from the transactions.
    pub fn apply_tipset_messages(
        &mut self,
        tipset: &FullTipset,
        mut callback: Option<impl FnMut(Cid, UnsignedMessage, ApplyRet) -> Result<(), String>>,
    ) -> Result<Vec<MessageReceipt>, Box<dyn StdError>> {
        let mut receipts = Vec::new();
        let mut processed = HashSet::<Cid>::default();

        for block in tipset.blocks() {
            let mut penalty = BigInt::zero();
            let mut gas_reward = BigInt::zero();

            let mut process_msg = |msg: &UnsignedMessage| -> Result<(), Box<dyn StdError>> {
                let cid = msg.cid()?;
                // Ensure no duplicate processing of a message
                if processed.contains(&cid) {
                    return Ok(());
                }
                let ret = self.apply_message(msg)?;

                // Update totals
                gas_reward += ret.miner_tip;
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
                miner: *block.header().miner_address(),
                penalty,
                gas_reward,
                // TODO revisit this if/when removed from go clients
                win_count: 1,
            })?;

            // TODO change this just just one get and update sequence in memory after interop
            let sys_act = self
                .state
                .get_actor(&*SYSTEM_ACTOR_ADDR)?
                .ok_or_else(|| "Failed to query system actor".to_string())?;

            let rew_msg = UnsignedMessage::builder()
                .from(*SYSTEM_ACTOR_ADDR)
                .to(*REWARD_ACTOR_ADDR)
                .sequence(sys_act.sequence)
                .value(BigInt::zero())
                .gas_premium(BigInt::zero())
                .gas_fee_cap(BigInt::zero())
                .gas_limit(1 << 30)
                .params(params)
                .method_num(reward::Method::AwardBlockReward as u64)
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

            if let Some(callback) = &mut callback {
                callback(rew_msg.cid()?, rew_msg, ret)?;
            }
        }

        // TODO same as above, unnecessary state retrieval
        let sys_act = self
            .state
            .get_actor(&*SYSTEM_ACTOR_ADDR)?
            .ok_or_else(|| "Failed to query system actor".to_string())?;

        let cron_msg = UnsignedMessage::builder()
            .from(*SYSTEM_ACTOR_ADDR)
            .to(*CRON_ACTOR_ADDR)
            .sequence(sys_act.sequence)
            .value(BigInt::zero())
            .gas_premium(BigInt::zero())
            .gas_fee_cap(BigInt::zero())
            .gas_limit(1 << 30)
            .method_num(cron::Method::EpochTick as u64)
            .params(Serialized::default())
            .build()?;

        let ret = self.apply_implicit_message(&cron_msg);
        if let Some(err) = ret.act_error {
            return Err(format!("failed to apply block cron message: {}", err).into());
        }

        if let Some(mut callback) = callback {
            callback(cron_msg.cid()?, cron_msg, ret)?;
        }
        Ok(receipts)
    }

    pub fn apply_implicit_message(&mut self, msg: &UnsignedMessage) -> ApplyRet {
        let (return_data, _, act_err) = self.send(msg, None);

        ApplyRet {
            msg_receipt: MessageReceipt {
                return_data,
                exit_code: if let Some(err) = act_err {
                    err.exit_code()
                } else {
                    ExitCode::Ok
                },
                gas_used: 0,
            },
            act_error: None,
            penalty: BigInt::zero(),
            miner_tip: BigInt::zero(),
        }
    }

    /// Applies the state transition for a single message
    /// Returns ApplyRet structure which contains the message receipt and some meta data.
    pub fn apply_message(&mut self, msg: &UnsignedMessage) -> Result<ApplyRet, String> {
        check_message(msg)?;

        let pl = price_list_by_epoch(self.epoch());
        let ser_msg = &msg.marshal_cbor().map_err(|e| e.to_string())?;
        let msg_gas_cost = pl.on_chain_message(ser_msg.len());

        if msg_gas_cost > msg.gas_limit() {
            return Ok(ApplyRet {
                msg_receipt: MessageReceipt {
                    return_data: Serialized::default(),
                    exit_code: ExitCode::SysErrOutOfGas,
                    gas_used: 0,
                },
                act_error: Some(actor_error!(SysErrOutOfGas;
                    "Out of gas ({} > {})", msg_gas_cost, msg.gas_limit())),
                penalty: &self.base_fee * msg_gas_cost,
                miner_tip: BigInt::zero(),
            });
        }

        let miner_penalty_amount = &self.base_fee * msg.gas_limit();
        let mut from_act = match self.state.get_actor(msg.from()) {
            Ok(from_act) => from_act.ok_or("Failed to retrieve actor state")?,
            Err(_) => {
                return Ok(ApplyRet {
                    msg_receipt: MessageReceipt {
                        return_data: Serialized::default(),
                        exit_code: ExitCode::SysErrSenderInvalid,
                        gas_used: 0,
                    },
                    penalty: miner_penalty_amount,
                    act_error: Some(actor_error!(SysErrSenderInvalid; "Sender invalid")),
                    miner_tip: 0.into(),
                });
            }
        };

        if from_act.code != *ACCOUNT_ACTOR_CODE_ID {
            return Ok(ApplyRet {
                msg_receipt: MessageReceipt {
                    return_data: Serialized::default(),
                    exit_code: ExitCode::SysErrSenderInvalid,
                    gas_used: 0,
                },
                penalty: miner_penalty_amount,
                act_error: Some(actor_error!(SysErrSenderInvalid; "send not from account actor")),
                miner_tip: 0.into(),
            });
        };

        // TODO revisit if this is removed in future
        if msg.sequence() != from_act.sequence {
            return Ok(ApplyRet {
                msg_receipt: MessageReceipt {
                    return_data: Serialized::default(),
                    exit_code: ExitCode::SysErrSenderStateInvalid,
                    gas_used: 0,
                },
                penalty: miner_penalty_amount,
                act_error: Some(actor_error!(SysErrSenderStateInvalid;
                    "actor sequence invalid: {} != {}", msg.sequence(), from_act.sequence)),
                miner_tip: 0.into(),
            });
        };

        let gas_cost = msg.gas_fee_cap() * msg.gas_limit();
        let total_cost = &gas_cost + msg.value();
        if from_act.balance < total_cost {
            return Ok(ApplyRet {
                msg_receipt: MessageReceipt {
                    return_data: Serialized::default(),
                    exit_code: ExitCode::SysErrSenderStateInvalid,
                    gas_used: 0,
                },
                penalty: miner_penalty_amount,
                act_error: Some(actor_error!(SysErrSenderStateInvalid;
                    "actor balance less than needed: {} < {}", from_act.balance, total_cost)),
                miner_tip: 0.into(),
            });
        };

        self.state.mutate_actor(msg.from(), |act| {
            from_act.deduct_funds(&gas_cost)?;
            act.sequence += 1;
            Ok(())
        })?;

        let snapshot = self.state.snapshot()?;

        let (mut ret_data, rt, mut act_err) = self.send(msg, Some(msg_gas_cost));
        if let Some(err) = &act_err {
            if err.is_fatal() {
                return Err(format!(
                    "[from={}, to={}, seq={}, m={}, h={}] fatal error: {}",
                    msg.from(),
                    msg.to(),
                    msg.sequence(),
                    msg.method_num(),
                    self.epoch,
                    err
                ));
            } else {
                warn!(
                    "[from={}, to={}, seq={}, m={}] send error: {}",
                    msg.from(),
                    msg.to(),
                    msg.sequence(),
                    msg.method_num(),
                    err
                );
                if !ret_data.is_empty() {
                    return Err(format!(
                        "message invocation errored, but had a return value anyway: {}",
                        err
                    ));
                }
            }
        }

        let gas_used = if let Some(mut rt) = rt {
            if !ret_data.is_empty() {
                if let Err(e) = rt.charge_gas(rt.price_list().on_chain_return_value(ret_data.len()))
                {
                    act_err = Some(e);
                    ret_data = Serialized::default();
                }
            }
            if rt.gas_used() < 0 {
                0
            } else {
                rt.gas_used()
            }
        } else {
            return Err(format!("send returned None runtime: {:?}", act_err));
        };

        if let Some(err) = &act_err {
            if !err.is_ok() {
                // Revert all state changes on error.
                self.state.revert_to_snapshot(&snapshot)?;
            }
        }

        let gas_outputs = compute_gas_outputs(
            gas_used,
            msg.gas_limit(),
            &self.base_fee,
            msg.gas_fee_cap(),
            msg.gas_premium().clone(),
        );
        self.state.mutate_actor(&*BURNT_FUNDS_ACTOR_ADDR, |act| {
            act.deposit_funds(&gas_outputs.base_fee_burn);
            Ok(())
        })?;
        self.state.mutate_actor(&*BURNT_FUNDS_ACTOR_ADDR, |act| {
            act.deposit_funds(&gas_outputs.over_estimation_burn);
            Ok(())
        })?;

        // refund unused gas
        self.state.mutate_actor(msg.from(), |act| {
            act.deposit_funds(&gas_outputs.refund);
            Ok(())
        })?;

        self.state.mutate_actor(&*REWARD_ACTOR_ADDR, |act| {
            act.deposit_funds(&gas_outputs.miner_tip);
            Ok(())
        })?;

        if &gas_outputs.base_fee_burn
            + gas_outputs.over_estimation_burn
            + &gas_outputs.refund
            + &gas_outputs.miner_tip
            != gas_cost
        {
            return Err("Gas handling math is wrong".to_owned());
        }

        Ok(ApplyRet {
            msg_receipt: MessageReceipt {
                return_data: ret_data,
                exit_code: ExitCode::Ok,
                gas_used,
            },
            penalty: gas_outputs.miner_penalty,
            act_error: None,
            miner_tip: gas_outputs.miner_tip,
        })
    }
    /// Instantiates a new Runtime, and calls internal_send to do the execution.
    fn send<'m>(
        &mut self,
        msg: &'m UnsignedMessage,
        gas_cost: Option<i64>,
    ) -> (
        Serialized,
        Option<DefaultRuntime<'db, 'm, '_, '_, '_, DB, SYS, P>>,
        Option<ActorError>,
    ) {
        let res = DefaultRuntime::new(
            &mut self.state,
            self.store,
            &self.syscalls,
            gas_cost.unwrap_or_default(),
            &msg,
            self.epoch,
            *msg.from(),
            msg.sequence(),
            0,
            self.rand,
        );

        match res {
            Ok(mut rt) => match vm_send(&mut rt, msg, gas_cost) {
                Ok(ser) => (ser, Some(rt), None),
                Err(actor_err) => (Serialized::default(), Some(rt), Some(actor_err)),
            },
            Err(e) => (Serialized::default(), None, Some(e)),
        }
    }
}

#[derive(Clone, Default)]
struct GasOutputs {
    pub base_fee_burn: TokenAmount,
    pub over_estimation_burn: TokenAmount,
    pub miner_penalty: TokenAmount,
    pub miner_tip: TokenAmount,
    pub refund: TokenAmount,

    pub gas_refund: i64,
    pub gas_burned: i64,
}

fn compute_gas_outputs(
    gas_used: i64,
    gas_limit: i64,
    base_fee: &TokenAmount,
    fee_cap: &TokenAmount,
    gas_premium: TokenAmount,
) -> GasOutputs {
    let gas_used_big: BigInt = 0.into();
    let mut base_fee_to_pay = base_fee;
    let mut out = GasOutputs::default();

    if base_fee > fee_cap {
        base_fee_to_pay = fee_cap;
        out.miner_penalty = (base_fee - fee_cap) * gas_used_big
    }
    let mut miner_tip = gas_premium;
    if &(base_fee_to_pay + &miner_tip) > fee_cap {
        miner_tip = fee_cap - base_fee_to_pay;
    }
    out.miner_tip = &miner_tip * gas_limit;
    let (out_gas_refund, out_gas_burned) = compute_gas_overestimation_burn(gas_used, gas_limit);
    out.gas_refund = out_gas_refund;
    out.gas_burned = out_gas_burned;

    if out.gas_burned != 0 {
        out.over_estimation_burn = base_fee_to_pay * out.gas_burned;
        out.miner_penalty += (base_fee - base_fee_to_pay) * out.gas_burned;
    }
    let required_funds = fee_cap * gas_limit;
    let refund = required_funds - &out.base_fee_burn - &out.miner_tip - &out.over_estimation_burn;
    out.refund = refund;

    out
}

pub fn compute_gas_overestimation_burn(gas_used: i64, gas_limit: i64) -> (i64, i64) {
    if gas_used == 0 {
        return (0, gas_limit);
    }

    let mut over = gas_limit - (GAS_OVERUSE_NUM * gas_used) / GAS_OVERUSE_DENOM;
    if over < 0 {
        return (gas_limit - gas_used, 0);
    }

    if over > gas_used {
        over = gas_used;
    }

    let mut gas_to_burn: BigInt = (gas_limit - gas_used).into();
    gas_to_burn *= over;
    gas_to_burn /= gas_used;

    (
        i64::try_from(gas_limit - gas_used - &gas_to_burn).unwrap(),
        i64::try_from(gas_to_burn).unwrap(),
    )
}

/// Apply message return data
#[derive(Clone)]
pub struct ApplyRet {
    pub msg_receipt: MessageReceipt,
    pub act_error: Option<ActorError>,
    pub penalty: BigInt,
    pub miner_tip: BigInt,
}

/// Does some basic checks on the Message to see if the fields are valid.
fn check_message(msg: &UnsignedMessage) -> Result<(), &'static str> {
    if msg.gas_limit() == 0 {
        return Err("Message has no gas limit set");
    }
    if msg.gas_limit() < 0 {
        return Err("Message has negative gas limit");
    }

    Ok(())
}
