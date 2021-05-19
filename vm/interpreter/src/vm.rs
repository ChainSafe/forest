// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{
    gas_tracker::{price_list_by_epoch, GasCharge},
    DefaultRuntime, Rand,
};
use actor::{
    actorv0::reward::AwardBlockRewardParams, cron, miner, reward, system, BURNT_FUNDS_ACTOR_ADDR,
};
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use fil_types::BLOCK_GAS_LIMIT;
use fil_types::{
    verifier::{FullVerifier, ProofVerifier},
    DefaultNetworkParams, NetworkParams, NetworkVersion,
};
use forest_encoding::Cbor;
use ipld_blockstore::BlockStore;
use log::debug;
use message::{ChainMessage, Message, MessageReceipt, UnsignedMessage};
use networks::{UPGRADE_ACTORS_V4_HEIGHT, UPGRADE_CLAUS_HEIGHT};
use num_bigint::{BigInt, Sign};
use num_traits::Zero;
use state_migration::nv12;
use state_tree::StateTree;
use std::collections::HashSet;
use std::convert::TryFrom;
use std::error::Error as StdError;
use std::marker::PhantomData;
use vm::{actor_error, ActorError, ExitCode, Serialized, TokenAmount};

const GAS_OVERUSE_NUM: i64 = 11;
const GAS_OVERUSE_DENOM: i64 = 10;

/// Contains all messages to process through the VM as well as miner information for block rewards.
#[derive(Debug)]
pub struct BlockMessages {
    pub miner: Address,
    pub messages: Vec<ChainMessage>,
    pub win_count: i64,
}

/// Allows generation of the current circulating supply
/// given some context.
pub trait CircSupplyCalc {
    /// Retrieves total circulating supply on the network.
    fn get_supply<DB: BlockStore>(
        &self,
        height: ChainEpoch,
        state_tree: &StateTree<DB>,
    ) -> Result<TokenAmount, Box<dyn StdError>>;
}

/// Trait to allow VM to retrieve state at an old epoch.
pub trait LookbackStateGetter<'db, DB> {
    /// Returns a state tree from the given epoch.
    fn state_lookback(&self, epoch: ChainEpoch) -> Result<StateTree<'db, DB>, Box<dyn StdError>>;
}

/// Interpreter which handles execution of state transitioning messages and returns receipts
/// from the vm execution.
pub struct VM<'db, 'r, DB, R, N, C, LB, V = FullVerifier, P = DefaultNetworkParams> {
    state: StateTree<'db, DB>,
    store: &'db DB,
    epoch: ChainEpoch,
    rand: &'r R,
    base_fee: BigInt,
    registered_actors: HashSet<Cid>,
    network_version_getter: N,
    circ_supply_calc: &'r C,
    lb_state: &'r LB,
    verifier: PhantomData<V>,
    params: PhantomData<P>,
}

impl<'db, 'r, DB, R, N, C, LB, V, P> VM<'db, 'r, DB, R, N, C, LB, V, P>
where
    DB: BlockStore,
    V: ProofVerifier,
    P: NetworkParams,
    R: Rand,
    N: Fn(ChainEpoch) -> NetworkVersion,
    C: CircSupplyCalc,
    LB: LookbackStateGetter<'db, DB>,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        root: &Cid,
        store: &'db DB,
        epoch: ChainEpoch,
        rand: &'r R,
        base_fee: BigInt,
        network_version_getter: N,
        circ_supply_calc: &'r C,
        lb_state: &'r LB,
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
            lb_state,
            verifier: PhantomData,
            params: PhantomData,
        })
    }

    /// Registers an actor that is not part of the set of default builtin actors by providing the
    /// code cid.
    pub fn register_actor(&mut self, code_cid: Cid) -> bool {
        self.registered_actors.insert(code_cid)
    }

    /// Gets registered actors that are not part of the set of default builtin actors.
    pub fn registered_actors(&self) -> &HashSet<Cid> {
        &self.registered_actors
    }

    /// Flush stores in VM and return state root.
    pub fn flush(&mut self) -> Result<Cid, Box<dyn StdError>> {
        self.state.flush()
    }

    /// Returns the epoch the VM is initialized with.
    fn epoch(&self) -> ChainEpoch {
        self.epoch
    }

    /// Returns a reference to the VM's state tree.
    pub fn state(&self) -> &StateTree<'_, DB> {
        &self.state
    }

    fn run_cron(
        &mut self,
        epoch: ChainEpoch,
        callback: Option<&mut impl FnMut(&Cid, &ChainMessage, &ApplyRet) -> Result<(), String>>,
    ) -> Result<(), Box<dyn StdError>> {
        let cron_msg = UnsignedMessage {
            from: **system::ADDRESS,
            to: **cron::ADDRESS,
            // Epoch as sequence is intentional
            sequence: epoch as u64,
            // Arbitrarily large gas limit for cron (matching Lotus value)
            gas_limit: BLOCK_GAS_LIMIT * 10000,
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
            callback(&cron_msg.cid()?, &ChainMessage::Unsigned(cron_msg), &ret)?;
        }
        Ok(())
    }

    /// Flushes the StateTree and perform a state migration if there is a migration at this epoch.
    /// If there is no migration this function will return Ok(None).
    #[allow(unreachable_code, unused_variables)]
    pub fn migrate_state(
        &mut self,
        epoch: ChainEpoch,
        arc_store: std::sync::Arc<impl BlockStore + Send + Sync>,
    ) -> Result<Option<Cid>, Box<dyn StdError>> {
        match epoch {
            x if x == UPGRADE_ACTORS_V4_HEIGHT => {
                let start = std::time::Instant::now();
                log::info!("Running actors_v4 state migration");
                // need to flush since we run_cron before the migration
                let prev_state = self.flush()?;
                let new_state = nv12::migrate_state_tree(arc_store, prev_state, epoch)?;
                if new_state != prev_state {
                    log::info!(
                        "actors_v4 state migration successful, took: {}ms",
                        start.elapsed().as_millis()
                    );
                    Ok(Some(new_state))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    /// Apply block messages from a Tipset.
    /// Returns the receipts from the transactions.
    pub fn apply_block_messages(
        &mut self,
        messages: &[BlockMessages],
        parent_epoch: ChainEpoch,
        epoch: ChainEpoch,
        arc_store: std::sync::Arc<impl BlockStore + Send + Sync>,
        mut callback: Option<impl FnMut(&Cid, &ChainMessage, &ApplyRet) -> Result<(), String>>,
    ) -> Result<Vec<MessageReceipt>, Box<dyn StdError>> {
        let mut receipts = Vec::new();
        let mut processed = HashSet::<Cid>::default();

        for i in parent_epoch..epoch {
            if i > parent_epoch {
                self.run_cron(epoch, callback.as_mut())?;
            }
            if let Some(new_state) = self.migrate_state(i, arc_store.clone())? {
                self.state = StateTree::new_from_root(self.store, &new_state)?
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
                    cb(&cid, msg, &ret)?;
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
            let params = Serialized::serialize(AwardBlockRewardParams {
                miner: block.miner,
                penalty,
                gas_reward,
                win_count: block.win_count,
            })?;

            let rew_msg = UnsignedMessage {
                from: **system::ADDRESS,
                to: **reward::ADDRESS,
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
                callback(&rew_msg.cid()?, &ChainMessage::Unsigned(rew_msg), &ret)?;
            }
        }

        self.run_cron(epoch, callback.as_mut())?;
        Ok(receipts)
    }

    /// Applies single message through vm and returns result from execution.
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

    /// Applies the state transition for a single message.
    /// Returns ApplyRet structure which contains the message receipt and some meta data.
    pub fn apply_message(&mut self, msg: &ChainMessage) -> Result<ApplyRet, String> {
        check_message(msg.message())?;

        let pl = price_list_by_epoch(self.epoch());
        let ser_msg = msg.marshal_cbor().map_err(|e| e.to_string())?;
        let msg_gas_cost = pl.on_chain_message(ser_msg.len());
        let cost_total = msg_gas_cost.total();

        // Verify the cost of the message is not over the message gas limit.
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

        // Load from actor state.
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

        // If from actor is not an account actor, return error.
        if !actor::is_account_actor(&from_act.code) {
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

        // Check sequence is correct
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

        // Ensure from actor has enough balance to cover the gas cost of the message.
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

        // Deduct gas cost and increment sequence
        self.state
            .mutate_actor(msg.from(), |act| {
                act.deduct_funds(&gas_cost)?;
                act.sequence += 1;
                Ok(())
            })
            .map_err(|e| e.to_string())?;

        self.state.snapshot()?;

        // Perform transaction
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
                debug!(
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

        let should_burn = self
            .should_burn(self.state(), msg, err_code)
            .map_err(|e| format!("failed to decide whether to burn: {}", e))?;

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
            should_burn,
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

        transfer_to_actor(&**reward::ADDRESS, &miner_tip)?;

        transfer_to_actor(&*BURNT_FUNDS_ACTOR_ADDR, &over_estimation_burn)?;

        // refund unused gas
        transfer_to_actor(msg.from(), &refund)?;

        if &base_fee_burn + over_estimation_burn + &refund + &miner_tip != gas_cost {
            // Sanity check. This could be a fatal error.
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
        Option<DefaultRuntime<'db, '_, DB, R, C, LB, V, P>>,
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
            0,
            self.rand,
            &self.registered_actors,
            self.circ_supply_calc,
            self.lb_state,
        );

        match res {
            Ok(mut rt) => match rt.send(msg, gas_cost) {
                Ok(ser) => (ser, Some(rt), None),
                Err(actor_err) => (Serialized::default(), Some(rt), Some(actor_err)),
            },
            Err(e) => (Serialized::default(), None, Some(e)),
        }
    }

    fn should_burn(
        &self,
        st: &StateTree<DB>,
        msg: &ChainMessage,
        exit_code: ExitCode,
    ) -> Result<bool, Box<dyn StdError>> {
        // Check to see if we should burn funds. We avoid burning on successful
        // window post. This won't catch _indirect_ window post calls, but this
        // is the best we can get for now.
        if self.epoch > UPGRADE_CLAUS_HEIGHT
            && exit_code.is_success()
            && msg.method_num() == miner::Method::SubmitWindowedPoSt as u64
        {
            // Ok, we've checked the _method_, but we still need to check
            // the target actor.
            let to_actor = st.get_actor(msg.to())?;

            if let Some(actor) = to_actor {
                if actor::is_miner_actor(&actor.code) {
                    // This is a storage miner and processed a window post, remove burn
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

#[derive(Clone, Default)]
struct GasOutputs {
    base_fee_burn: TokenAmount,
    over_estimation_burn: TokenAmount,
    miner_penalty: TokenAmount,
    miner_tip: TokenAmount,
    refund: TokenAmount,

    gas_refund: i64,
    gas_burned: i64,
}

fn compute_gas_outputs(
    gas_used: i64,
    gas_limit: i64,
    base_fee: &TokenAmount,
    fee_cap: &TokenAmount,
    gas_premium: TokenAmount,
    charge_network_fee: bool,
) -> GasOutputs {
    let mut base_fee_to_pay = base_fee;
    let mut out = GasOutputs::default();

    if base_fee > fee_cap {
        base_fee_to_pay = fee_cap;
        out.miner_penalty = (base_fee - fee_cap) * gas_used
    }

    // If charge network fee is disabled just skip computing the base fee burn.
    // This is part of the temporary fix with Claus fork.
    if charge_network_fee {
        out.base_fee_burn = base_fee_to_pay * gas_used;
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

fn compute_gas_overestimation_burn(gas_used: i64, gas_limit: i64) -> (i64, i64) {
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

    let gas_to_burn = i64::try_from(gas_to_burn).unwrap();
    (gas_limit - gas_used - gas_to_burn, gas_to_burn)
}

/// Apply message return data.
#[derive(Clone, Debug)]
pub struct ApplyRet {
    /// Message receipt for the transaction. This data is stored on chain.
    pub msg_receipt: MessageReceipt,
    /// Actor error from the transaction, if one exists.
    pub act_error: Option<ActorError>,
    /// Gas penalty from transaction, if any.
    pub penalty: BigInt,
    /// Tip given to miner from message.
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
