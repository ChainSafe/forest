// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use forest_actor_interface::{reward, system, AwardBlockRewardParams};
use forest_ipld_blockstore::BlockStore;
use forest_message::ChainMessage;
use forest_networks::{ChainConfig, Height};
use fvm::state_tree::StateTree;
use fvm_ipld_encoding::RawBytes;
use fvm_shared::address::Address;
use fvm_shared::bigint::BigInt;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::econ::TokenAmount;
use fvm_shared::message::Message;
use fvm_shared::METHOD_SEND;

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
        penalty: BigInt,
        gas_reward: BigInt,
    ) -> Result<Option<Message>, anyhow::Error>;
}

/// Trait to allow VM to retrieve state at an old epoch.
pub trait LookbackStateGetter {
    /// Returns the root CID for a given `ChainEpoch`
    fn chain_epoch_root(&self) -> Box<dyn Fn(ChainEpoch) -> Cid>;
}

#[derive(Clone, Copy)]
pub struct Heights {
    pub calico: ChainEpoch,
    pub turbo: ChainEpoch,
    pub hyperdrive: ChainEpoch,
    pub chocolate: ChainEpoch,
}

impl Heights {
    pub fn new(chain_config: &ChainConfig) -> Self {
        Heights {
            calico: chain_config.epoch(Height::Calico),
            turbo: chain_config.epoch(Height::Turbo),
            hyperdrive: chain_config.epoch(Height::Hyperdrive),
            chocolate: chain_config.epoch(Height::Chocolate),
        }
    }
}

/// Does some basic checks on the Message to see if the fields are valid.
pub fn check_message(msg: &Message) -> Result<(), anyhow::Error> {
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
        penalty: BigInt,
        gas_reward: BigInt,
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
        _penalty: BigInt,
        _gas_reward: BigInt,
    ) -> Result<Option<Message>, anyhow::Error> {
        Ok(None)
    }
}

/// Giving a fixed amount of coins for each block produced directly to the miner,
/// on top of the gas spent, so the circulating supply isn't burned. Ignores penalties.
pub struct FixedRewardCalc {
    pub reward: BigInt,
}

impl RewardCalc for FixedRewardCalc {
    fn reward_message(
        &self,
        epoch: ChainEpoch,
        miner: Address,
        _win_count: i64,
        _penalty: BigInt,
        gas_reward: BigInt,
    ) -> Result<Option<Message>, anyhow::Error> {
        let msg = Message {
            from: reward::ADDRESS,
            to: miner,
            method_num: METHOD_SEND as u64,
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
