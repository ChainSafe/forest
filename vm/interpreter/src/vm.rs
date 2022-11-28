// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::fvm::ForestExterns;
use cid::Cid;
use forest_actor_interface::{cron, reward, system, AwardBlockRewardParams};
use forest_message::ChainMessage;
use forest_networks::ChainConfig;
use fvm::executor::{ApplyRet, DefaultExecutor};
use fvm::externs::Rand;
use fvm::machine::{DefaultMachine, Machine, MultiEngine, NetworkConfig};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{Cbor, RawBytes};
use fvm_shared::address::Address;
use fvm_shared::bigint::Zero;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::econ::TokenAmount;
use fvm_shared::error::ExitCode;
use fvm_shared::message::Message;
use fvm_shared::receipt::Receipt;
use fvm_shared::{BLOCK_GAS_LIMIT, METHOD_SEND};
use std::collections::HashSet;
use std::sync::Arc;

pub(crate) type ForestMachine<DB> = DefaultMachine<DB, ForestExterns<DB>>;

#[cfg(not(feature = "instrumented_kernel"))]
type ForestKernel<DB> =
    fvm::DefaultKernel<fvm::call_manager::DefaultCallManager<ForestMachine<DB>>>;

#[cfg(not(feature = "instrumented_kernel"))]
type ForestExecutor<DB> = DefaultExecutor<ForestKernel<DB>>;

#[cfg(feature = "instrumented_kernel")]
type ForestExecutor<DB> = DefaultExecutor<crate::instrumented_kernel::ForestInstrumentedKernel<DB>>;

/// Contains all messages to process through the VM as well as miner information for block rewards.
#[derive(Debug)]
pub struct BlockMessages {
    pub miner: Address,
    pub messages: Vec<ChainMessage>,
    pub win_count: i64,
}

/// Allows the generation of a reward message based on gas fees and penalties.
///
/// This should facilitate custom consensus protocols using their own economic incentives.
pub trait RewardCalc: Send + Sync + 'static {
    /// Construct a reward message, if rewards are applicable.
    fn reward_message(
        &self,
        epoch: ChainEpoch,
        miner: Address,
        win_count: i64,
        penalty: TokenAmount,
        gas_reward: TokenAmount,
    ) -> Result<Option<Message>, anyhow::Error>;
}

/// Interpreter which handles execution of state transitioning messages and returns receipts
/// from the VM execution.
pub struct VM<DB: Blockstore + 'static> {
    fvm_executor: ForestExecutor<DB>,
    reward_calc: Arc<dyn RewardCalc>,
}

impl<DB> VM<DB>
where
    DB: Blockstore + Clone,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        root: Cid,
        store: DB,
        epoch: ChainEpoch,
        rand: impl Rand + 'static,
        base_fee: TokenAmount,
        circ_supply: TokenAmount,
        reward_calc: Arc<dyn RewardCalc>,
        lb_fn: Box<dyn Fn(ChainEpoch) -> Cid>,
        multi_engine: &MultiEngine,
        chain_config: Arc<ChainConfig>,
    ) -> Result<Self, anyhow::Error> {
        let network_version = chain_config.network_version(epoch);
        let config = NetworkConfig::new(network_version);
        let engine = multi_engine.get(&config)?;
        let mut context = config.for_epoch(epoch, root);
        context.set_base_fee(base_fee);
        context.set_circulating_supply(circ_supply);
        let fvm: fvm::machine::DefaultMachine<DB, ForestExterns<DB>> =
            fvm::machine::DefaultMachine::new(
                &engine,
                &context,
                store.clone(),
                ForestExterns::new(rand, epoch, root, lb_fn, store, chain_config),
            )?;
        let exec: ForestExecutor<DB> = DefaultExecutor::new(fvm);
        Ok(VM {
            fvm_executor: exec,
            reward_calc,
        })
    }

    /// Flush stores in VM and return state root.
    pub fn flush(&mut self) -> anyhow::Result<Cid> {
        Ok(self.fvm_executor.flush()?)
    }

    /// Get actor state from an address. Will be resolved to ID address.
    pub fn get_actor(
        &self,
        addr: &Address,
    ) -> Result<Option<fvm::state_tree::ActorState>, anyhow::Error> {
        Ok(self.fvm_executor.state_tree().get_actor(addr)?)
    }

    pub fn run_cron(
        &mut self,
        epoch: ChainEpoch,
        callback: Option<
            &mut impl FnMut(&Cid, &ChainMessage, &ApplyRet) -> Result<(), anyhow::Error>,
        >,
    ) -> Result<(), anyhow::Error> {
        let cron_msg = Message {
            from: system::ADDRESS,
            to: cron::ADDRESS,
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

        let ret = self.apply_implicit_message(&cron_msg)?;
        if let Some(err) = ret.failure_info {
            anyhow::bail!("failed to apply block cron message: {}", err);
        }

        if let Some(callback) = callback {
            callback(&(cron_msg.cid()?), &ChainMessage::Unsigned(cron_msg), &ret)?;
        }
        Ok(())
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
    ) -> Result<Vec<Receipt>, anyhow::Error> {
        let mut receipts = Vec::new();
        let mut processed = HashSet::<Cid>::default();

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
            if let Some(rew_msg) = self.reward_calc.reward_message(
                epoch,
                block.miner,
                block.win_count,
                penalty,
                gas_reward,
            )? {
                let ret = self.apply_implicit_message(&rew_msg)?;
                if let Some(err) = ret.failure_info {
                    anyhow::bail!(
                        "failed to apply reward message for miner {}: {}",
                        block.miner,
                        err
                    );
                }
                // This is more of a sanity check, this should not be able to be hit.
                if ret.msg_receipt.exit_code != ExitCode::OK {
                    anyhow::bail!(
                        "reward application message failed (exit: {:?})",
                        ret.msg_receipt.exit_code
                    );
                }
                if let Some(callback) = &mut callback {
                    callback(&(rew_msg.cid()?), &ChainMessage::Unsigned(rew_msg), &ret)?;
                }
            }
        }

        if let Err(e) = self.run_cron(epoch, callback.as_mut()) {
            log::error!("End of epoch cron failed to run: {}", e);
        }
        Ok(receipts)
    }

    /// Applies single message through VM and returns result from execution.
    pub fn apply_implicit_message(&mut self, msg: &Message) -> Result<ApplyRet, anyhow::Error> {
        use fvm::executor::Executor;
        // raw_length is not used for Implicit messages.
        let raw_length = msg.marshal_cbor().expect("encoding error").len();
        let ret = self.fvm_executor.execute_message(
            msg.clone(),
            fvm::executor::ApplyKind::Implicit,
            raw_length,
        )?;
        Ok(ret)
    }

    /// Applies the state transition for a single message.
    /// Returns `ApplyRet` structure which contains the message receipt and some meta data.
    pub fn apply_message(&mut self, msg: &ChainMessage) -> Result<ApplyRet, anyhow::Error> {
        check_message(msg.message())?;

        use fvm::executor::Executor;
        let unsigned = msg.message().clone();
        let raw_length = msg.marshal_cbor().expect("encoding error").len();
        let ret = self.fvm_executor.execute_message(
            unsigned,
            fvm::executor::ApplyKind::Explicit,
            raw_length,
        )?;

        let exit_code = ret.msg_receipt.exit_code;

        if !exit_code.is_success() {
            match exit_code.value() {
                1..=ExitCode::FIRST_USER_EXIT_CODE => {
                    log::debug!(
                        "Internal message execution failure. Exit code was {}",
                        exit_code
                    )
                }
                _ => {
                    log::warn!("Message execution failed with exit code {}", exit_code)
                }
            };
        }

        Ok(ret)
    }
}

/// Does some basic checks on the Message to see if the fields are valid.
fn check_message(msg: &Message) -> Result<(), anyhow::Error> {
    if msg.gas_limit == 0 {
        anyhow::bail!("Message has no gas limit set");
    }
    if msg.gas_limit < 0 {
        anyhow::bail!("Message has negative gas limit");
    }

    Ok(())
}

/// Default reward working with the Filecoin Reward Actor.
pub struct RewardActorMessageCalc;

impl RewardCalc for RewardActorMessageCalc {
    fn reward_message(
        &self,
        epoch: ChainEpoch,
        miner: Address,
        win_count: i64,
        penalty: TokenAmount,
        gas_reward: TokenAmount,
    ) -> Result<Option<Message>, anyhow::Error> {
        let params = RawBytes::serialize(AwardBlockRewardParams {
            miner,
            penalty,
            gas_reward,
            win_count,
        })?;

        let rew_msg = Message {
            from: system::ADDRESS,
            to: reward::ADDRESS,
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

        Ok(Some(rew_msg))
    }
}

/// Not giving any reward for block creation.
pub struct NoRewardCalc;

impl RewardCalc for NoRewardCalc {
    fn reward_message(
        &self,
        _epoch: ChainEpoch,
        _miner: Address,
        _win_count: i64,
        _penalty: TokenAmount,
        _gas_reward: TokenAmount,
    ) -> Result<Option<Message>, anyhow::Error> {
        Ok(None)
    }
}

/// Giving a fixed amount of coins for each block produced directly to the miner,
/// on top of the gas spent, so the circulating supply isn't burned. Ignores penalties.
pub struct FixedRewardCalc {
    pub reward: TokenAmount,
}

impl RewardCalc for FixedRewardCalc {
    fn reward_message(
        &self,
        epoch: ChainEpoch,
        miner: Address,
        _win_count: i64,
        _penalty: TokenAmount,
        gas_reward: TokenAmount,
    ) -> Result<Option<Message>, anyhow::Error> {
        let msg = Message {
            from: reward::ADDRESS,
            to: miner,
            method_num: METHOD_SEND,
            params: Default::default(),
            // Epoch as sequence is intentional
            sequence: epoch as u64,
            gas_limit: 1 << 30,
            value: gas_reward + self.reward.clone(),
            version: Default::default(),
            gas_fee_cap: Default::default(),
            gas_premium: Default::default(),
        };

        Ok(Some(msg))
    }
}
