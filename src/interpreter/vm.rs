// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::blocks::Tipset;
use crate::chain::index::ChainIndex;
use crate::message::ChainMessage;
use crate::networks::{ChainConfig, NetworkChain};
use crate::shim::{
    address::Address,
    econ::TokenAmount,
    executor::{build_lotus_trace, ApplyRet, ExecutionEvent_v3, Receipt, Trace},
    externs::{Rand, RandWrapper},
    machine::MultiEngine,
    message::{Message, Message_v3},
    state_tree::ActorState,
    version::NetworkVersion,
};
use ahash::HashSet;
use anyhow::bail;
use cid::Cid;
use fil_actor_interface::{cron, reward, AwardBlockRewardParams};
use fvm2::{
    executor::{DefaultExecutor as DefaultExecutor_v2, Executor as Executor_v2},
    machine::{
        DefaultMachine as DefaultMachine_v2, Machine as Machine_v2,
        NetworkConfig as NetworkConfig_v2,
    },
};
use fvm3::{
    executor::{DefaultExecutor as DefaultExecutor_v3, Executor as Executor_v3},
    machine::{
        DefaultMachine as DefaultMachine_v3, Machine as Machine_v3,
        NetworkConfig as NetworkConfig_v3,
    },
};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{to_vec, RawBytes};
use fvm_shared2::{clock::ChainEpoch, BLOCK_GAS_LIMIT};
use num::Zero;
use num_bigint::BigInt;

use crate::interpreter::{fvm2::ForestExternsV2, fvm3::ForestExterns as ForestExternsV3};

pub(in crate::interpreter) type ForestMachineV2<DB> =
    DefaultMachine_v2<Arc<DB>, ForestExternsV2<DB>>;
pub(in crate::interpreter) type ForestMachineV3<DB> =
    DefaultMachine_v3<Arc<DB>, ForestExternsV3<DB>>;

type ForestKernelV2<DB> =
    fvm2::DefaultKernel<fvm2::call_manager::DefaultCallManager<ForestMachineV2<DB>>>;
type ForestKernelV3<DB> =
    fvm3::DefaultKernel<fvm3::call_manager::DefaultCallManager<ForestMachineV3<DB>>>;
type ForestExecutorV2<DB> = DefaultExecutor_v2<ForestKernelV2<DB>>;
type ForestExecutorV3<DB> = DefaultExecutor_v3<ForestKernelV3<DB>>;

/// Contains all messages to process through the VM as well as miner information
/// for block rewards.
#[derive(Debug)]
pub struct BlockMessages {
    pub miner: Address,
    pub messages: Vec<ChainMessage>,
    pub win_count: i64,
}

#[derive(Clone, Debug)]
pub struct MessageGasCost {
    pub message: Cid,
    pub gas_used: BigInt,
    pub base_fee_burn: TokenAmount,
    pub over_estimation_burn: TokenAmount,
    pub miner_penalty: TokenAmount,
    pub miner_tip: TokenAmount,
    pub refund: TokenAmount,
    pub total_cost: TokenAmount,
}

impl MessageGasCost {
    pub fn new(msg: &Message, ret: ApplyRet) -> Self {
        use crate::message::Message as MessageTrait;
        Self {
            message: msg.cid().unwrap(),
            gas_used: BigInt::from(ret.msg_receipt().gas_used()),
            base_fee_burn: ret.base_fee_burn(),
            over_estimation_burn: ret.over_estimation_burn(),
            miner_penalty: ret.penalty(),
            miner_tip: ret.miner_tip(),
            refund: ret.refund(),
            total_cost: msg.required_funds() - &ret.refund(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct InvocResult {
    pub msg_cid: Cid,
    pub msg: Message,
    pub msg_receipt: Receipt,
    pub gas_cost: MessageGasCost,
    pub execution_trace: Option<Trace>,
    pub error: String,
}

fn build_exec_trace(exec_trace: Vec<ExecutionEvent_v3>) -> Option<Trace> {
    let exec_trace: Option<Trace> = if !exec_trace.is_empty() {
        let mut trace_iter = exec_trace.into_iter();
        let mut initial_gas_charges = Vec::new();
        loop {
            match trace_iter.next() {
                Some(gc @ ExecutionEvent_v3::GasCharge(_)) => initial_gas_charges.push(gc),
                Some(ExecutionEvent_v3::Call {
                    from,
                    to,
                    method,
                    params,
                    value,
                }) => {
                    break build_lotus_trace(
                        from,
                        to.into(),
                        method,
                        params.clone(),
                        value.into(),
                        &mut initial_gas_charges.into_iter().chain(&mut trace_iter),
                    )
                    .ok()
                }
                // Skip anything unexpected.
                Some(_) => {}
                // Return none if we don't even have a call.
                None => break None,
            }
        }
    } else {
        None
    };

    exec_trace
}

/// Interpreter which handles execution of state transitioning messages and
/// returns receipts from the VM execution.
pub enum VM<DB: Blockstore + Send + Sync + 'static> {
    VM2(ForestExecutorV2<DB>),
    VM3(ForestExecutorV3<DB>),
}

pub struct ExecutionContext<DB> {
    // This tipset identifies of the blockchain. It functions as a starting
    // point when searching for ancestors. It may be any tipset as long as its
    // epoch is at or higher than the epoch in `epoch`.
    pub heaviest_tipset: Arc<Tipset>,
    // State-tree generated by the parent tipset.
    pub state_tree_root: Cid,
    // Epoch of the messages to be executed.
    pub epoch: ChainEpoch,
    // Source of deterministic randomness
    pub rand: Box<dyn Rand>,
    // https://spec.filecoin.io/systems/filecoin_vm/gas_fee/
    pub base_fee: TokenAmount,
    // https://filecoin.io/blog/filecoin-circulating-supply/
    pub circ_supply: TokenAmount,
    // The chain config is used to determine which consensus rules to use.
    pub chain_config: Arc<ChainConfig>,
    // Caching interface to the DB
    pub chain_index: Arc<ChainIndex<Arc<DB>>>,
    // UNIX timestamp for epoch
    pub timestamp: u64,
}

impl<DB> VM<DB>
where
    DB: Blockstore + Send + Sync,
{
    pub fn new(
        ExecutionContext {
            heaviest_tipset,
            state_tree_root,
            epoch,
            rand,
            base_fee,
            circ_supply,
            chain_config,
            chain_index,
            timestamp,
        }: ExecutionContext<DB>,
        multi_engine: &MultiEngine,
        enable_tracing: bool,
    ) -> Result<Self, anyhow::Error> {
        let network_version = chain_config.network_version(epoch);
        if network_version >= NetworkVersion::V18 {
            let mut config = NetworkConfig_v3::new(network_version.into());
            // ChainId defines the chain ID used in the Ethereum JSON-RPC endpoint.
            config.chain_id(chain_config.eth_chain_id.into());
            if let NetworkChain::Devnet(_) = chain_config.network {
                config.enable_actor_debugging();
            }

            let engine = multi_engine.v3.get(&config)?;
            let mut context = config.for_epoch(epoch, timestamp, state_tree_root);
            context.set_base_fee(base_fee.into());
            context.set_circulating_supply(circ_supply.into());

            if enable_tracing {
                context.enable_tracing();
            }
            let fvm: ForestMachineV3<DB> = ForestMachineV3::new(
                &context,
                Arc::clone(&chain_index.db),
                ForestExternsV3::new(
                    RandWrapper::from(rand),
                    heaviest_tipset,
                    epoch,
                    state_tree_root,
                    chain_index,
                    chain_config,
                ),
            )?;
            let exec: ForestExecutorV3<DB> = DefaultExecutor_v3::new(engine, fvm)?;
            Ok(VM::VM3(exec))
        } else {
            let config = NetworkConfig_v2::new(network_version.into());
            let engine = multi_engine.v2.get(&config)?;
            let mut context = config.for_epoch(epoch, state_tree_root);
            context.set_base_fee(base_fee.into());
            context.set_circulating_supply(circ_supply.into());

            if enable_tracing {
                context.enable_tracing();
            }
            let fvm: ForestMachineV2<DB> = ForestMachineV2::new(
                &engine,
                &context,
                Arc::clone(&chain_index.db),
                ForestExternsV2::new(
                    RandWrapper::from(rand),
                    heaviest_tipset,
                    epoch,
                    state_tree_root,
                    chain_index,
                    chain_config,
                ),
            )?;
            let exec: ForestExecutorV2<DB> = DefaultExecutor_v2::new(fvm);
            Ok(VM::VM2(exec))
        }
    }

    /// Flush stores in VM and return state root.
    pub fn flush(&mut self) -> anyhow::Result<Cid> {
        match self {
            VM::VM2(fvm_executor) => Ok(fvm_executor.flush()?),
            VM::VM3(fvm_executor) => Ok(fvm_executor.flush()?),
        }
    }

    /// Get actor state from an address. Will be resolved to ID address.
    pub fn get_actor(&self, addr: &Address) -> Result<Option<ActorState>, anyhow::Error> {
        match self {
            VM::VM2(fvm_executor) => Ok(fvm_executor
                .state_tree()
                .get_actor(&addr.into())?
                .map(ActorState::from)),
            VM::VM3(fvm_executor) => {
                if let Some(id) = fvm_executor.state_tree().lookup_id(&addr.into())? {
                    Ok(fvm_executor
                        .state_tree()
                        .get_actor(id)?
                        .map(ActorState::from))
                } else {
                    Ok(None)
                }
            }
        }
    }

    pub fn run_cron(
        &mut self,
        epoch: ChainEpoch,
        callback: Option<
            &mut impl FnMut(&Cid, &ChainMessage, &ApplyRet) -> Result<(), anyhow::Error>,
        >,
    ) -> Result<(Message, ApplyRet), anyhow::Error> {
        let cron_msg: Message = Message_v3 {
            from: Address::SYSTEM_ACTOR.into(),
            to: Address::CRON_ACTOR.into(),
            // Epoch as sequence is intentional
            sequence: epoch as u64,
            // Arbitrarily large gas limit for cron (matching Lotus value)
            gas_limit: BLOCK_GAS_LIMIT as u64 * 10000,
            method_num: cron::Method::EpochTick as u64,
            params: Default::default(),
            value: Default::default(),
            version: Default::default(),
            gas_fee_cap: Default::default(),
            gas_premium: Default::default(),
        }
        .into();

        let ret = self.apply_implicit_message(&cron_msg)?;
        if let Some(err) = ret.failure_info() {
            anyhow::bail!("failed to apply block cron message: {}", err);
        }

        if let Some(callback) = callback {
            callback(
                &(cron_msg.cid()?),
                &ChainMessage::Unsigned(cron_msg.clone()),
                &ret,
            )?;
        }
        Ok((cron_msg, ret))
    }

    /// Apply block messages from a Tipset.
    /// Returns the receipts from the transactions.
    pub fn apply_block_messages(
        &mut self,
        messages: &[BlockMessages],
        epoch: ChainEpoch,
        mut callback: Option<
            impl FnMut(&Cid, &ChainMessage, &ApplyRet) -> Result<(), anyhow::Error>,
        >,
        enable_tracing: bool,
    ) -> Result<(Vec<Receipt>, Vec<InvocResult>), anyhow::Error> {
        let mut receipts = Vec::new();
        let mut processed = HashSet::<Cid>::default();
        let mut invoc_results = Vec::new();

        for block in messages.iter() {
            let mut penalty = TokenAmount::zero();
            let mut gas_reward = TokenAmount::zero();

            let mut process_msg = |msg: &ChainMessage| -> Result<(), anyhow::Error> {
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
                gas_reward += ret.miner_tip();
                penalty += ret.penalty();
                let msg_receipt = ret.msg_receipt();
                receipts.push(msg_receipt.clone());

                // Push InvocResult
                if enable_tracing {
                    let trace = build_exec_trace(ret.exec_trace());

                    invoc_results.push(InvocResult {
                        msg_cid: cid,
                        msg: msg.message().clone(),
                        msg_receipt,
                        gas_cost: MessageGasCost::new(msg.message(), ret.clone()),
                        execution_trace: trace,
                        error: ret.failure_info().unwrap_or_default(),
                    });
                }

                // Add processed Cid to set of processed messages
                processed.insert(cid);
                Ok(())
            };

            for msg in block.messages.iter() {
                process_msg(msg)?;
            }

            // Generate reward transaction for the miner of the block
            if let Some(rew_msg) =
                self.reward_message(epoch, block.miner, block.win_count, penalty, gas_reward)?
            {
                let ret = self.apply_implicit_message(&rew_msg)?;
                if let Some(err) = ret.failure_info() {
                    anyhow::bail!(
                        "failed to apply reward message for miner {}: {}",
                        block.miner,
                        err
                    );
                }
                // This is more of a sanity check, this should not be able to be hit.
                if !ret.msg_receipt().exit_code().is_success() {
                    anyhow::bail!(
                        "reward application message failed (exit: {:?})",
                        ret.msg_receipt().exit_code()
                    );
                }

                // Push InvocResult
                if enable_tracing {
                    let trace = build_exec_trace(ret.exec_trace());

                    invoc_results.push(InvocResult {
                        msg_cid: rew_msg.cid()?,
                        msg: rew_msg.clone(),
                        msg_receipt: ret.msg_receipt(),
                        gas_cost: MessageGasCost::new(&rew_msg, ret.clone()),
                        execution_trace: trace,
                        error: ret.failure_info().unwrap_or_default(),
                    });
                }

                if let Some(callback) = &mut callback {
                    callback(&(rew_msg.cid()?), &ChainMessage::Unsigned(rew_msg), &ret)?;
                }
            }
        }

        match self.run_cron(epoch, callback.as_mut()) {
            Ok((cron_msg, ret)) => {
                // Push InvocResult
                if enable_tracing {
                    let trace = build_exec_trace(ret.exec_trace());

                    invoc_results.push(InvocResult {
                        msg_cid: cron_msg.cid()?,
                        msg: cron_msg.clone(),
                        msg_receipt: ret.msg_receipt(),
                        gas_cost: MessageGasCost::new(&cron_msg, ret.clone()),
                        execution_trace: trace,
                        error: ret.failure_info().unwrap_or_default(),
                    });
                }
            }
            Err(e) => {
                tracing::error!("End of epoch cron failed to run: {}", e);
            }
        }

        Ok((receipts, invoc_results))
    }

    /// Applies single message through VM and returns result from execution.
    pub fn apply_implicit_message(&mut self, msg: &Message) -> Result<ApplyRet, anyhow::Error> {
        // raw_length is not used for Implicit messages.
        let raw_length = to_vec(msg).expect("encoding error").len();

        match self {
            VM::VM2(fvm_executor) => {
                let ret = fvm_executor.execute_message(
                    msg.into(),
                    fvm2::executor::ApplyKind::Implicit,
                    raw_length,
                )?;
                Ok(ret.into())
            }
            VM::VM3(fvm_executor) => {
                let ret = fvm_executor.execute_message(
                    msg.into(),
                    fvm3::executor::ApplyKind::Implicit,
                    raw_length,
                )?;
                Ok(ret.into())
            }
        }
    }

    /// Applies the state transition for a single message.
    /// Returns `ApplyRet` structure which contains the message receipt and some
    /// meta data.
    pub fn apply_message(&mut self, msg: &ChainMessage) -> Result<ApplyRet, anyhow::Error> {
        // Basic validity check
        msg.message().check()?;

        let unsigned = msg.message().clone();
        let raw_length = to_vec(msg).expect("encoding error").len();
        let ret: ApplyRet = match self {
            VM::VM2(fvm_executor) => {
                let ret = fvm_executor.execute_message(
                    unsigned.into(),
                    fvm2::executor::ApplyKind::Explicit,
                    raw_length,
                )?;

                if fvm_executor.externs().bail() {
                    bail!("encountered a database lookup error");
                }

                ret.into()
            }
            VM::VM3(fvm_executor) => {
                let ret = fvm_executor.execute_message(
                    unsigned.into(),
                    fvm3::executor::ApplyKind::Explicit,
                    raw_length,
                )?;

                if fvm_executor.externs().bail() {
                    bail!("encountered a database lookup error");
                }

                ret.into()
            }
        };

        let exit_code = ret.msg_receipt().exit_code();

        if !exit_code.is_success() {
            tracing::debug!(?exit_code, "VM message execution failure.")
        }

        Ok(ret)
    }

    fn reward_message(
        &self,
        epoch: ChainEpoch,
        miner: Address,
        win_count: i64,
        penalty: TokenAmount,
        gas_reward: TokenAmount,
    ) -> Result<Option<Message>, anyhow::Error> {
        let params = RawBytes::serialize(AwardBlockRewardParams {
            miner: miner.into(),
            penalty: penalty.into(),
            gas_reward: gas_reward.into(),
            win_count,
        })?;
        let rew_msg = Message_v3 {
            from: Address::SYSTEM_ACTOR.into(),
            to: Address::REWARD_ACTOR.into(),
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
        Ok(Some(rew_msg.into()))
    }
}
