// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::types::*;
use clock::ChainEpoch;
use encoding::{repr::*, tuple::*, Cbor};
use fil_types::{Spacetime, StoragePower};
use num_bigint::bigint_ser;
use num_bigint::biguint_ser;
use num_derive::FromPrimitive;
use vm::TokenAmount;

/// Reward actor state
#[derive(Serialize_tuple, Deserialize_tuple, Default)]
pub struct State {
    #[serde(with = "bigint_ser")]
    pub baseline_power: StoragePower,
    #[serde(with = "bigint_ser")]
    pub realized_power: StoragePower,
    #[serde(with = "biguint_ser")]
    pub cumsum_baseline: Spacetime,
    #[serde(with = "biguint_ser")]
    pub cumsum_realized: Spacetime,
    #[serde(with = "biguint_ser")]
    pub effective_network_time: NetworkTime,

    #[serde(with = "bigint_ser")]
    pub simple_supply: TokenAmount,
    #[serde(with = "bigint_ser")]
    pub baseline_supply: TokenAmount,

    /// The reward to be paid in total to block producers, if exactly the expected number of them produce a block.
    /// The actual reward total paid out depends on the number of winners in any round.
    /// This is computed at the end of the previous epoch, and should really be called ThisEpochReward.
    #[serde(with = "bigint_ser")]
    pub last_per_epoch_reward: TokenAmount,

    /// The count of epochs for which a reward has been paid.
    /// This should equal the number of non-empty tipsets after the genesis, aka "chain height".
    pub reward_epochs_paid: ChainEpoch,
}

impl State {
    pub fn new() -> Self {
        Self::default()
    }

    pub(super) fn get_effective_network_time(
        &self,
        _cumsum_baseline: &Spacetime,
        cumsum_realized: &Spacetime,
    ) -> NetworkTime {
        // TODO: this function depends on the final baseline
        // EffectiveNetworkTime is a fractional input with an implicit denominator of (2^MintingInputFixedPoint).
        // realizedCumsum is thus left shifted by MintingInputFixedPoint before converted into a FixedPoint fraction
        // through division (which is an inverse function for the integral of the baseline).
        (cumsum_realized << MINTING_INPUT_FIXED_POINT) / BASELINE_POWER
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
