// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::logic::*;
use super::types::*;
use crate::smooth::FilterEstimate;
use clock::{ChainEpoch, EPOCH_UNDEFINED};
use encoding::{repr::*, tuple::*, Cbor};
use fil_types::{Spacetime, StoragePower};
use num_bigint::bigint_ser;
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

    /// CumsumRealized is cumulative sum of network power capped by BalinePower(epoch).
    /// Expressed in byte-epochs.
    #[serde(with = "bigint_ser")]
    pub cumsum_realized: Spacetime,

    /// Ceiling of real effective network time `theta` based on
    /// CumsumBaselinePower(theta) == CumsumRealizedPower
    /// Theta captures the notion of how much the network has progressed in its baseline
    /// and in advancing network time.
    #[serde(with = "bigint_ser")]
    pub effective_network_time: NetworkTime,

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

    /// TotalMined tracks the total FIL awared to block miners.
    #[serde(with = "bigint_ser")]
    pub total_mined: TokenAmount,
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
            ..Default::default()
        };
        st.update_to_next_epoch_with_reward(&curr_realized_power);

        st
    }

    fn update_to_next_epoch(&mut self, curr_realized_power: &StoragePower) {
        todo!()
    }

    fn update_to_next_epoch_with_reward(&mut self, curr_realized_power: &StoragePower) {
        todo!()
    }

    fn update_smoothed_estimates(&mut self, delta: ChainEpoch) {
        todo!()
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
                    (self.value.clone() * elapsed as u64) / vest_duration as u64
                }
            }
        }
    }
}
