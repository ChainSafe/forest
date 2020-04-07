// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::Multimap;
use address::Address;
use cid::Cid;
use clock::ChainEpoch;
use encoding::Cbor;
use ipld_blockstore::BlockStore;
use num_bigint::biguint_ser::{BigUintDe, BigUintSer};
use num_derive::FromPrimitive;
use num_traits::{CheckedSub, FromPrimitive};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use vm::TokenAmount;

/// Reward actor state
pub struct State {
    /// Reward multimap indexing addresses.
    pub reward_map: Cid,
    /// Sum of un-withdrawn rewards.
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

        rewards.add(owner.hash_key(), reward)?;

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
        let key = owner.hash_key();

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
            rewards.add(key.clone(), rew)?;
        }

        // Update rewards multimap root and total
        self.reward_map = rewards.root()?;
        self.reward_total -= &withdrawable_sum;
        Ok(withdrawable_sum)
    }
}

impl Cbor for State {}
impl Serialize for State {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.reward_map, BigUintSer(&self.reward_total)).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for State {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (reward_map, BigUintDe(reward_total)) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            reward_map,
            reward_total,
        })
    }
}

/// Defines vestion function type for reward actor
#[derive(Clone, Debug, PartialEq, Copy, FromPrimitive)]
#[repr(u8)]
pub enum VestingFunction {
    None = 0,
    Linear = 1,
}

impl Serialize for VestingFunction {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (*self as u8).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for VestingFunction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let b: u8 = Deserialize::deserialize(deserializer)?;
        Ok(FromPrimitive::from_u8(b)
            .ok_or_else(|| de::Error::custom("Invalid registered proof byte"))?)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reward {
    pub vesting_function: VestingFunction,
    pub start_epoch: ChainEpoch,
    pub end_epoch: ChainEpoch,
    pub value: TokenAmount,
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

impl Serialize for Reward {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            &self.vesting_function,
            &self.start_epoch,
            &self.end_epoch,
            BigUintSer(&self.value),
            BigUintSer(&self.amount_withdrawn),
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Reward {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (
            vesting_function,
            start_epoch,
            end_epoch,
            BigUintDe(value),
            BigUintDe(amount_withdrawn),
        ) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            vesting_function,
            start_epoch,
            end_epoch,
            value,
            amount_withdrawn,
        })
    }
}
