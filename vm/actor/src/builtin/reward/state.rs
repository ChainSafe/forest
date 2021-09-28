// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::logic::*;
use crate::smooth::{AlphaBetaFilter, FilterEstimate, DEFAULT_ALPHA, DEFAULT_BETA};
use clock::{ChainEpoch, EPOCH_UNDEFINED};
use encoding::{repr::*, tuple::*, Cbor};
use fil_types::{Spacetime, StoragePower};
use num_bigint::{bigint_ser, Integer};
use num_derive::FromPrimitive;
use vm::TokenAmount;

lazy_static! {
    /// 36.266260308195979333 FIL
    pub static ref INITIAL_REWARD_POSITION_ESTIMATE: TokenAmount = TokenAmount::from(36266260308195979333u128);
    /// -1.0982489*10^-7 FIL per epoch.  Change of simple minted tokens between epochs 0 and 1.
    pub static ref INITIAL_REWARD_VELOCITY_ESTIMATE: TokenAmount = TokenAmount::from(-109897758509i64);
}

/// Reward actor state
#[derive(Serialize_tuple, Deserialize_tuple, Default)]
pub struct State {
    /// Target CumsumRealized needs to reach for EffectiveNetworkTime to increase
    /// Expressed in byte-epochs.
    #[serde(with = "bigint_ser")]
    pub cumsum_baseline: Spacetime,

    /// CumsumRealized is cumulative sum of network power capped by BaselinePower(epoch).
    /// Expressed in byte-epochs.
    #[serde(with = "bigint_ser")]
    pub cumsum_realized: Spacetime,

    /// Ceiling of real effective network time `theta` based on
    /// CumsumBaselinePower(theta) == CumsumRealizedPower
    /// Theta captures the notion of how much the network has progressed in its baseline
    /// and in advancing network time.
    pub effective_network_time: ChainEpoch,

    /// EffectiveBaselinePower is the baseline power at the EffectiveNetworkTime epoch.
    #[serde(with = "bigint_ser")]
    pub effective_baseline_power: StoragePower,

    /// The reward to be paid in per WinCount to block producers.
    /// The actual reward total paid out depends on the number of winners in any round.
    /// This value is recomputed every non-null epoch and used in the next non-null epoch.
    #[serde(with = "bigint_ser")]
    pub this_epoch_reward: TokenAmount,
    /// Smoothed `this_epoch_reward`.
    pub this_epoch_reward_smoothed: FilterEstimate,

    /// The baseline power the network is targeting at st.Epoch.
    #[serde(with = "bigint_ser")]
    pub this_epoch_baseline_power: StoragePower,

    /// Epoch tracks for which epoch the Reward was computed.
    pub epoch: ChainEpoch,

    // TotalStoragePowerReward tracks the total FIL awarded to block miners
    #[serde(with = "bigint_ser")]
    pub total_storage_power_reward: TokenAmount,

    // Simple and Baseline totals are constants used for computing rewards.
    // They are on chain because of a historical fix resetting baseline value
    // in a way that depended on the history leading immediately up to the
    // migration fixing the value.  These values can be moved from state back
    // into a code constant in a subsequent upgrade.
    #[serde(with = "bigint_ser")]
    pub simple_total: TokenAmount,
    #[serde(with = "bigint_ser")]
    pub baseline_total: TokenAmount,
}

impl State {
    pub fn new(curr_realized_power: StoragePower) -> Self {
        let mut st = Self {
            effective_baseline_power: BASELINE_INITIAL_VALUE.clone(),
            this_epoch_baseline_power: INIT_BASELINE_POWER.clone(),
            epoch: EPOCH_UNDEFINED,
            this_epoch_reward_smoothed: FilterEstimate::new(
                INITIAL_REWARD_POSITION_ESTIMATE.clone(),
                INITIAL_REWARD_VELOCITY_ESTIMATE.clone(),
            ),
            simple_total: SIMPLE_TOTAL.clone(),
            baseline_total: BASELINE_TOTAL.clone(),
            ..Default::default()
        };
        st.update_to_next_epoch_with_reward(&curr_realized_power);

        st
    }

    /// Takes in current realized power and updates internal state
    /// Used for update of internal state during null rounds
    pub(super) fn update_to_next_epoch(&mut self, curr_realized_power: &StoragePower) {
        self.epoch += 1;
        self.this_epoch_baseline_power = baseline_power_from_prev(&self.this_epoch_baseline_power);
        let capped_realized_power =
            std::cmp::min(&self.this_epoch_baseline_power, curr_realized_power);
        self.cumsum_realized += capped_realized_power;

        while self.cumsum_realized > self.cumsum_baseline {
            self.effective_network_time += 1;
            self.effective_baseline_power =
                baseline_power_from_prev(&self.effective_baseline_power);
            self.cumsum_baseline += &self.effective_baseline_power;
        }
    }

    /// Takes in a current realized power for a reward epoch and computes
    /// and updates reward state to track reward for the next epoch
    pub(super) fn update_to_next_epoch_with_reward(&mut self, curr_realized_power: &StoragePower) {
        let prev_reward_theta = compute_r_theta(
            self.effective_network_time,
            &self.effective_baseline_power,
            &self.cumsum_realized,
            &self.cumsum_baseline,
        );
        self.update_to_next_epoch(curr_realized_power);
        let curr_reward_theta = compute_r_theta(
            self.effective_network_time,
            &self.effective_baseline_power,
            &self.cumsum_realized,
            &self.cumsum_baseline,
        );

        self.this_epoch_reward = compute_reward(
            self.epoch,
            prev_reward_theta,
            curr_reward_theta,
            &self.simple_total,
            &self.baseline_total,
        );
    }

    pub(super) fn update_smoothed_estimates(&mut self, delta: ChainEpoch) {
        let filter_reward = AlphaBetaFilter::load(
            &self.this_epoch_reward_smoothed,
            &DEFAULT_ALPHA,
            &DEFAULT_BETA,
        );
        self.this_epoch_reward_smoothed =
            filter_reward.next_estimate(&self.this_epoch_reward, delta);
    }

    pub fn into_total_storage_power_reward(self) -> TokenAmount {
        self.total_storage_power_reward
    }
}

impl Cbor for State {}

/// Defines vestion function type for reward actor.
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
    #[serde(with = "bigint_ser")]
    pub value: TokenAmount,
    #[serde(with = "bigint_ser")]
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
                    (self.value.clone() * elapsed as u64)
                        .div_floor(&TokenAmount::from(vest_duration))
                }
            }
        }
    }
}
