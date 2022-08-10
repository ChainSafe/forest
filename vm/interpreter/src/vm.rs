// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::fvm::{ForestExterns, ForestKernel, ForestMachine};
use actor_interface::{cron, reward, system, AwardBlockRewardParams};
use cid::Cid;
use forest_message::{ChainMessage, MessageReceipt};
use forest_vm::{Serialized, TokenAmount};
use fvm::executor::ApplyRet;
use fvm::externs::Rand;
use fvm::machine::{Engine, Machine, NetworkConfig};
use fvm::state_tree::StateTree;
use fvm_ipld_encoding::Cbor;
use fvm_shared::address::Address;
use fvm_shared::bigint::BigInt;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::error::ExitCode;
use fvm_shared::message::Message;
use fvm_shared::version::NetworkVersion;
use fvm_shared::{DefaultNetworkParams, NetworkParams, BLOCK_GAS_LIMIT};
use ipld_blockstore::BlockStore;
use networks::{ChainConfig, Height};
use std::collections::HashSet;
use std::marker::PhantomData;
use std::sync::Arc;

// const GAS_OVERUSE_NUM: i64 = 11;
// const GAS_OVERUSE_DENOM: i64 = 10;

/// Contains all messages to process through the VM as well as miner information for block rewards.
#[derive(Debug)]
pub struct BlockMessages {
    pub miner: Address,
    pub messages: Vec<ChainMessage>,
    pub win_count: i64,
}

/// Allows generation of the current circulating supply
/// given some context.
pub trait CircSupplyCalc: Clone + 'static {
    /// Retrieves total circulating supply on the network.
    fn get_supply<DB: BlockStore>(
        &self,
        height: ChainEpoch,
        state_tree: &StateTree<DB>,
    ) -> Result<TokenAmount, anyhow::Error>;
}

/// Trait to allow VM to retrieve state at an old epoch.
pub trait LookbackStateGetter {
    /// Returns the root CID for a given `ChainEpoch`
    fn chain_epoch_root(&self) -> Box<dyn Fn(ChainEpoch) -> Cid>;
}

#[derive(Clone, Copy)]
pub struct Heights {
    pub calico: ChainEpoch,
    pub claus: ChainEpoch,
    pub turbo: ChainEpoch,
    pub hyperdrive: ChainEpoch,
    pub chocolate: ChainEpoch,
}

impl Heights {
    pub fn new(chain_config: &ChainConfig) -> Self {
        Heights {
            calico: chain_config.epoch(Height::Calico),
            claus: chain_config.epoch(Height::Claus),
            turbo: chain_config.epoch(Height::Turbo),
            hyperdrive: chain_config.epoch(Height::Hyperdrive),
            chocolate: chain_config.epoch(Height::Chocolate),
        }
    }
}

/// Interpreter which handles execution of state transitioning messages and returns receipts
/// from the VM execution.
pub struct VM<DB: BlockStore + 'static, P = DefaultNetworkParams> {
    fvm_executor: fvm::executor::DefaultExecutor<ForestKernel<DB>>,
    params: PhantomData<P>,
    heights: Heights,
}

impl<DB, P> VM<DB, P>
where
    DB: BlockStore,
    P: NetworkParams,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new<R, C, LB>(
        root: Cid,
        store_arc: DB,
        epoch: ChainEpoch,
        rand: &R,
        base_fee: BigInt,
        network_version: NetworkVersion,
        circ_supply_calc: C,
        override_circ_supply: Option<TokenAmount>,
        lb_state: &LB,
        engine: Engine,
        heights: Heights,
        chain_finality: i64,
    ) -> Result<Self, anyhow::Error>
    where
        R: Rand + Clone + 'static,
        C: CircSupplyCalc,
        LB: LookbackStateGetter,
    {
        let state = StateTree::new_from_root(&store_arc, &root)?;
        let circ_supply = circ_supply_calc.get_supply(epoch, &state).unwrap();

        let mut context = NetworkConfig::new(network_version).for_epoch(epoch, root);
        context.set_base_fee(base_fee);
        context.set_circulating_supply(circ_supply);
        context.enable_tracing();
        let fvm: fvm::machine::DefaultMachine<DB, ForestExterns<DB>> =
            fvm::machine::DefaultMachine::new(
                &engine,
                &context,
                store_arc.clone(),
                ForestExterns::new(
                    rand.clone(),
                    epoch,
                    root,
                    lb_state.chain_epoch_root(),
                    store_arc,
                    network_version,
                    chain_finality,
                ),
            )?;
        let exec: fvm::executor::DefaultExecutor<ForestKernel<DB>> =
            fvm::executor::DefaultExecutor::new(ForestMachine {
                machine: fvm,
                circ_supply: override_circ_supply,
            });
        Ok(VM {
            fvm_executor: exec,
            params: PhantomData,
            heights,
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

    /// Flushes the `StateTree` and perform a state migration if there is a migration at this epoch.
    /// If there is no migration this function will return `Ok(None)`.
    pub fn migrate_state(
        &self,
        epoch: ChainEpoch,
        _store: Arc<impl BlockStore + Send + Sync>,
    ) -> Result<Option<Cid>, anyhow::Error> {
        match epoch {
            x if x == self.heights.turbo => {
                // FIXME: Support state migrations.
                panic!("Cannot migrate state when using FVM. See https://github.com/ChainSafe/forest/issues/1454 for updates.");
            }
            _ => Ok(None),
        }
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
    ) -> Result<Vec<MessageReceipt>, anyhow::Error> {
        let mut receipts = Vec::new();
        let mut processed = HashSet::<Cid>::default();

        for block in messages.iter() {
            let mut penalty = Default::default();
            let mut gas_reward = Default::default();

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
            let params = Serialized::serialize(AwardBlockRewardParams {
                miner: block.miner,
                penalty,
                gas_reward,
                win_count: block.win_count,
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
