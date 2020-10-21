// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{
    gas_tracker::{price_list_by_epoch, GasCharge},
    vm_send, DefaultRuntime, Rand,
};
use actor::{
    cron, reward, ACCOUNT_ACTOR_CODE_ID, BURNT_FUNDS_ACTOR_ADDR, CRON_ACTOR_ADDR,
    REWARD_ACTOR_ADDR, SYSTEM_ACTOR_ADDR,
};
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use fil_types::{
    verifier::{FullVerifier, ProofVerifier},
    DevnetParams, NetworkParams, NetworkVersion,
};
use forest_encoding::Cbor;
use ipld_blockstore::BlockStore;
use log::warn;
use message::{ChainMessage, Message, MessageReceipt, UnsignedMessage};
use num_bigint::{BigInt, Sign};
use num_traits::Zero;
use state_tree::StateTree;
use std::collections::HashSet;
use std::convert::TryFrom;
use std::error::Error as StdError;
use std::marker::PhantomData;
use vm::{actor_error, ActorError, ExitCode, Serialized, TokenAmount};

const GAS_OVERUSE_NUM: i64 = 11;
const GAS_OVERUSE_DENOM: i64 = 10;

#[derive(Debug)]
pub struct BlockMessages {
    pub miner: Address,
    pub messages: Vec<ChainMessage>,
    pub win_count: i64,
}

// TODO replace with some trait or some generic solution (needs to use context)
pub type CircSupplyCalc<BS> =
    Box<dyn Fn(ChainEpoch, &StateTree<BS>) -> Result<TokenAmount, String>>;

/// Interpreter which handles execution of state transitioning messages and returns receipts
/// from the vm execution.
pub struct VM<'db, 'r, DB, R, N, V = FullVerifier, P = DevnetParams> {
    state: StateTree<'db, DB>,
    store: &'db DB,
    epoch: ChainEpoch,
    rand: &'r R,
    base_fee: BigInt,
    registered_actors: HashSet<Cid>,
    network_version_getter: N,
    circ_supply_calc: Option<CircSupplyCalc<DB>>,
    verifier: PhantomData<V>,
    params: PhantomData<P>,
}

impl<'db, 'r, DB, R, N, V, P> VM<'db, 'r, DB, R, N, V, P>
where
    DB: BlockStore,
    V: ProofVerifier,
    P: NetworkParams,
    R: Rand,
    N: Fn(ChainEpoch) -> NetworkVersion,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        root: &Cid,
        store: &'db DB,
        epoch: ChainEpoch,
        rand: &'r R,
        base_fee: BigInt,
        network_version_getter: N,
        circ_supply_calc: Option<CircSupplyCalc<DB>>,
    ) -> Result<Self, String> {
        let state = StateTree::new_from_root(store, root).map_err(|e| e.to_string())?;
        let registered_actors = HashSet::new();
        Ok(VM {
            network_version_getter,
            state,
            store,
            epoch,
            rand,
            base_fee,
            registered_actors,
            circ_supply_calc,
            verifier: PhantomData,
            params: PhantomData,
        })
    }

    /// Registers an actor that is not part of the set of default builtin actors by providing the code cid
    pub fn register_actor(&mut self, code_cid: Cid) -> bool {
        self.registered_actors.insert(code_cid)
    }

    /// Gets registered actors that are not part of the set of default builtin actors
    pub fn registered_actors(&self) -> &HashSet<Cid> {
        &self.registered_actors
    }

    /// Flush stores in VM and return state root.
    pub fn flush(&mut self) -> Result<Cid, String> {
        self.state.flush().map_err(|e| e.to_string())
    }

    /// Returns ChainEpoch
    fn epoch(&self) -> ChainEpoch {
        self.epoch
    }

    pub fn state(&self) -> &StateTree<'_, DB> {
        &self.state
    }

    fn run_cron(
        &mut self,
        epoch: ChainEpoch,
        callback: Option<&mut impl FnMut(Cid, &ChainMessage, ApplyRet) -> Result<(), String>>,
    ) -> Result<(), Box<dyn StdError>> {
        let cron_msg = UnsignedMessage {
            from: *SYSTEM_ACTOR_ADDR,
            to: *CRON_ACTOR_ADDR,
            // Epoch as sequence is intentional
            sequence: epoch as u64,
            gas_limit: 1 << 30,
            method_num: cron::Method::EpochTick as u64,
            params: Default::default(),
            value: Default::default(),
            version: Default::default(),
            gas_fee_cap: Default::default(),
            gas_premium: Default::default(),
        };

        let ret = self.apply_implicit_message(&cron_msg);
        if let Some(err) = ret.act_error {
            return Err(format!("failed to apply block cron message: {}", err).into());
        }

        if let Some(callback) = callback {
            callback(cron_msg.cid()?, &ChainMessage::Unsigned(cron_msg), ret)?;
        }
        Ok(())
    }

    /// Apply block messages from a Tipset.
    /// Returns the receipts from the transactions.
    pub fn apply_block_messages(
        &mut self,
        messages: &[BlockMessages],
        parent_epoch: ChainEpoch,
        epoch: ChainEpoch,
        mut callback: Option<impl FnMut(Cid, &ChainMessage, ApplyRet) -> Result<(), String>>,
    ) -> Result<Vec<MessageReceipt>, Box<dyn StdError>> {
        let mut receipts = Vec::new();
        let mut processed = HashSet::<Cid>::default();

        for i in parent_epoch..epoch {
            if i > parent_epoch {
                self.run_cron(epoch, callback.as_mut())?;
            }
            self.epoch = i + 1;
        }

        for block in messages.iter() {
            let mut penalty = Default::default();
            let mut gas_reward = Default::default();

            let mut process_msg = |msg: &ChainMessage| -> Result<(), Box<dyn StdError>> {
                let cid = msg.cid()?;
                // Ensure no duplicate processing of a message
                if processed.contains(&cid) {
                    return Ok(());
                }
                let ret = self.apply_message(msg)?;
                if let Some(cb) = &mut callback {
                    cb(msg.cid()?, msg, ret.clone())?;
                }

                // Update totals
                gas_reward += &ret.miner_tip;
                penalty += &ret.penalty;
                receipts.push(ret.msg_receipt);

                // Add processed Cid to set of processed messages
                processed.insert(cid);
                Ok(())
            };

            for msg in block.messages.iter() {
                process_msg(msg)?;
            }

            // Generate reward transaction for the miner of the block
            let params = Serialized::serialize(reward::AwardBlockRewardParams {
                miner: block.miner,
                penalty,
                gas_reward,
                win_count: block.win_count,
            })?;

            let rew_msg = UnsignedMessage {
                from: *SYSTEM_ACTOR_ADDR,
                to: *REWARD_ACTOR_ADDR,
                method_num: reward::Method::AwardBlockReward as u64,
                params,
                // Epoch as sequence is intentional
                sequence: epoch as u64,
                gas_limit: 1 << 30,
                value: Default::default(),
                version: Default::default(),
                gas_fee_cap: Default::default(),
                gas_premium: Default::default(),
            };

            let ret = self.apply_implicit_message(&rew_msg);
            if let Some(err) = ret.act_error {
                return Err(format!(
                    "failed to apply reward message for miner {}: {}",
                    block.miner, err
                )
                .into());
            }

            // This is more of a sanity check, this should not be able to be hit.
            if ret.msg_receipt.exit_code != ExitCode::Ok {
                return Err(format!(
                    "reward application message failed (exit: {:?})",
                    ret.msg_receipt.exit_code
                )
                .into());
            }

            if let Some(callback) = &mut callback {
                callback(rew_msg.cid()?, &ChainMessage::Unsigned(rew_msg), ret)?;
            }
        }

        self.run_cron(epoch, callback.as_mut())?;
        Ok(receipts)
    }

    pub fn apply_implicit_message(&mut self, msg: &UnsignedMessage) -> ApplyRet {
        let (return_data, _, act_err) = self.send(msg, None);

        ApplyRet {
            msg_receipt: MessageReceipt {
                return_data,
                exit_code: if let Some(err) = &act_err {
                    err.exit_code()
                } else {
                    ExitCode::Ok
                },
                gas_used: 0,
            },
            act_error: act_err,
            penalty: BigInt::zero(),
            miner_tip: BigInt::zero(),
        }
    }

    /// Applies the state transition for a single message
    /// Returns ApplyRet structure which contains the message receipt and some meta data.
    pub fn apply_message(&mut self, msg: &ChainMessage) -> Result<ApplyRet, String> {
        check_message(msg.message())?;

        let pl = price_list_by_epoch(self.epoch());
        let ser_msg = msg.marshal_cbor().map_err(|e| e.to_string())?;
        let msg_gas_cost = pl.on_chain_message(ser_msg.len());
        let cost_total = msg_gas_cost.total();

        if cost_total > msg.gas_limit() {
            return Ok(ApplyRet {
                msg_receipt: MessageReceipt {
                    return_data: Serialized::default(),
                    exit_code: ExitCode::SysErrOutOfGas,
                    gas_used: 0,
                },
                act_error: Some(actor_error!(SysErrOutOfGas;
                    "Out of gas ({} > {})", cost_total, msg.gas_limit())),
                penalty: &self.base_fee * cost_total,
                miner_tip: BigInt::zero(),
            });
        }

        let miner_penalty_amount = &self.base_fee * msg.gas_limit();
        let from_act = match self.state.get_actor(msg.from()) {
            Ok(Some(from_act)) => from_act,
            _ => {
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

        let gas_cost: TokenAmount = msg.gas_fee_cap() * msg.gas_limit();
        if from_act.balance < gas_cost {
            return Ok(ApplyRet {
                msg_receipt: MessageReceipt {
                    return_data: Serialized::default(),
                    exit_code: ExitCode::SysErrSenderStateInvalid,
                    gas_used: 0,
                },
                penalty: miner_penalty_amount,
                act_error: Some(actor_error!(SysErrSenderStateInvalid;
                    "actor balance less than needed: {} < {}", from_act.balance, gas_cost)),
                miner_tip: 0.into(),
            });
        };

        self.state
            .mutate_actor(msg.from(), |act| {
                act.deduct_funds(&gas_cost)?;
                act.sequence += 1;
                Ok(())
            })
            .map_err(|e| e.to_string())?;

        self.state.snapshot()?;

        let (mut ret_data, rt, mut act_err) = self.send(msg.message(), Some(msg_gas_cost));
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

        let err_code = if let Some(err) = &act_err {
            if !err.is_ok() {
                // Revert all state changes on error.
                self.state.revert_to_snapshot()?;
            }
            err.exit_code()
        } else {
            ExitCode::Ok
        };

        let GasOutputs {
            base_fee_burn,
            miner_tip,
            over_estimation_burn,
            refund,
            miner_penalty,
            ..
        } = compute_gas_outputs(
            gas_used,
            msg.gas_limit(),
            &self.base_fee,
            msg.gas_fee_cap(),
            msg.gas_premium().clone(),
        );

        let mut transfer_to_actor = |addr: &Address, amt: &TokenAmount| -> Result<(), String> {
            if amt.sign() == Sign::Minus {
                return Err("attempted to transfer negative value into actor".into());
            }
            if amt.is_zero() {
                return Ok(());
            }

            self.state
                .mutate_actor(addr, |act| {
                    act.deposit_funds(&amt);
                    Ok(())
                })
                .map_err(|e| e.to_string())?;
            Ok(())
        };

        transfer_to_actor(&*BURNT_FUNDS_ACTOR_ADDR, &base_fee_burn)?;

        transfer_to_actor(&*REWARD_ACTOR_ADDR, &miner_tip)?;

        transfer_to_actor(&*BURNT_FUNDS_ACTOR_ADDR, &over_estimation_burn)?;

        // refund unused gas
        transfer_to_actor(msg.from(), &refund)?;

        if &base_fee_burn + over_estimation_burn + &refund + &miner_tip != gas_cost {
            return Err("Gas handling math is wrong".to_owned());
        }
        self.state.clear_snapshot()?;

        Ok(ApplyRet {
            msg_receipt: MessageReceipt {
                return_data: ret_data,
                exit_code: err_code,
                gas_used,
            },
            penalty: miner_penalty,
            act_error: act_err,
            miner_tip,
        })
    }

    /// Instantiates a new Runtime, and calls vm_send to do the execution.
    #[allow(clippy::type_complexity)]
    fn send(
        &mut self,
        msg: &UnsignedMessage,
        gas_cost: Option<GasCharge>,
    ) -> (
        Serialized,
        Option<DefaultRuntime<'db, '_, DB, R, V, P>>,
        Option<ActorError>,
    ) {
        let res = DefaultRuntime::new(
            (self.network_version_getter)(self.epoch),
            &mut self.state,
            self.store,
            0,
            &msg,
            self.epoch,
            *msg.from(),
            msg.sequence(),
            0,
            self.rand,
            &self.registered_actors,
            &self.circ_supply_calc,
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
    let mut base_fee_to_pay = base_fee;
    let mut out = GasOutputs::default();

    if base_fee > fee_cap {
        base_fee_to_pay = fee_cap;
        out.miner_penalty = (base_fee - fee_cap) * gas_used
    }
    out.base_fee_burn = base_fee_to_pay * gas_used;

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
#[derive(Clone, Debug)]
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
