// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::Multimap;
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use encoding::{repr::*, tuple::*, Cbor};
use ipld_blockstore::BlockStore;
use num_bigint::biguint_ser;
use num_derive::FromPrimitive;
use num_traits::CheckedSub;
use vm::TokenAmount;

/// Reward actor state
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct State {
    /// Reward multimap indexing addresses.
    pub reward_map: Cid,
    /// Sum of un-withdrawn rewards.
    #[serde(with = "biguint_ser")]
    pub reward_total: TokenAmount,
}

impl State {
    pub fn new(empty_multimap: Cid) -> Self {
        Self {
            reward_map: empty_multimap,
            reward_total: TokenAmount::default(),
        }
    }

    #[allow(dead_code)]
    pub(super) fn add_reward<BS: BlockStore>(
        &mut self,
        store: &BS,
        owner: &Address,
        reward: Reward,
    ) -> Result<(), String> {
        let mut rewards = Multimap::from_root(store, &self.reward_map)?;
        let value = reward.value.clone();

        rewards.add(owner.to_bytes().into(), reward)?;

        self.reward_map = rewards.root()?;
        self.reward_total += value;
        Ok(())
    }

    /// Calculates and subtracts the total withdrawable reward for an owner.
    #[allow(dead_code)]
    pub(super) fn withdraw_reward<BS: BlockStore>(
        &mut self,
        store: &BS,
        owner: &Address,
        curr_epoch: ChainEpoch,
    ) -> Result<TokenAmount, String> {
        let mut rewards = Multimap::from_root(store, &self.reward_map)?;
        let key = owner.to_bytes();

        // Iterate rewards, accumulate total and remaining reward state
        let mut remaining_rewards = Vec::new();
        let mut withdrawable_sum = TokenAmount::from(0u8);
        rewards.for_each(&key, |_, reward: &Reward| {
            let unlocked = reward.amount_vested(curr_epoch);
            let withdrawable = unlocked
                .checked_sub(&reward.amount_withdrawn)
                .ok_or(format!(
                    "Unlocked amount {} less than amount withdrawn {} at epoch {}",
                    unlocked, reward.amount_withdrawn, curr_epoch
                ))?;

            withdrawable_sum += withdrawable;
            if unlocked < reward.value {
                remaining_rewards.push(Reward {
                    vesting_function: reward.vesting_function,
                    start_epoch: reward.start_epoch,
                    end_epoch: reward.end_epoch,
                    value: reward.value.clone(),
                    amount_withdrawn: unlocked,
                });
            }
            Ok(())
        })?;

        assert!(
            withdrawable_sum < self.reward_total,
            "withdrawable amount cannot exceed previous total"
        );

        // Regenerate amt for multimap with updated rewards
        rewards.remove_all(&key)?;
        for rew in remaining_rewards {
            rewards.add(key.clone().into(), rew)?;
        }

        // Update rewards multimap root and total
        self.reward_map = rewards.root()?;
        self.reward_total -= &withdrawable_sum;
        Ok(withdrawable_sum)
    }
}

impl Cbor for State {}

/// Defines vestion function type for reward actor
#[derive(Clone, Debug, PartialEq, Copy, FromPrimitive, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum VestingFunction {
    None = 0,
    Linear = 1,
}

#[derive(Clone, Debug, PartialEq, Serialize_tuple, Deserialize_tuple)]
pub struct Reward {
    pub vesting_function: VestingFunction,
    pub start_epoch: ChainEpoch,
    pub end_epoch: ChainEpoch,
    #[serde(with = "biguint_ser")]
    pub value: TokenAmount,
    #[serde(with = "biguint_ser")]
    pub amount_withdrawn: TokenAmount,
}

impl Reward {
    pub fn amount_vested(&self, curr_epoch: ChainEpoch) -> TokenAmount {
        match self.vesting_function {
            VestingFunction::None => self.value.clone(),
            VestingFunction::Linear => {
                let elapsed = curr_epoch - self.start_epoch;
                let vest_duration = self.end_epoch - self.start_epoch;
                if elapsed >= vest_duration {
                    self.value.clone()
                } else {
                    (self.value.clone() * elapsed) / vest_duration
                }
            }
        }
    }
}
