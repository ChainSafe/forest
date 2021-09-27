// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::VestSpec;
use clock::ChainEpoch;
use encoding::tuple::*;
use fil_types::deadlines::QuantSpec;
use num_bigint::{bigint_ser, Integer};
use num_traits::Zero;
use std::collections::HashMap;
use vm::TokenAmount;

// Represents miner funds that will vest at the given epoch.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub struct VestingFund {
    pub epoch: ChainEpoch,
    #[serde(with = "bigint_ser")]
    pub amount: TokenAmount,
}

/// Represents the vesting table state for the miner.
/// It is a slice of (VestingEpoch, VestingAmount).
/// The slice will always be sorted by the VestingEpoch.
#[derive(Serialize_tuple, Deserialize_tuple, Default)]
pub struct VestingFunds {
    pub funds: Vec<VestingFund>,
}

impl VestingFunds {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn unlock_vested_funds(&mut self, current_epoch: ChainEpoch) -> TokenAmount {
        // TODO: the funds are sorted by epoch, so we could do a binary search here
        let i = self
            .funds
            .iter()
            .position(|fund| fund.epoch >= current_epoch)
            .unwrap_or(self.funds.len());

        self.funds.drain(..i).map(|fund| fund.amount).sum()
    }

    pub fn add_locked_funds(
        &mut self,
        current_epoch: ChainEpoch,
        vesting_sum: &TokenAmount,
        proving_period_start: ChainEpoch,
        spec: &VestSpec,
    ) {
        // maps the epochs in VestingFunds to their indices in the vec
        let mut epoch_to_index = HashMap::<ChainEpoch, usize>::with_capacity(self.funds.len());

        for (i, fund) in self.funds.iter().enumerate() {
            epoch_to_index.insert(fund.epoch, i);
        }

        // Quantization is aligned with when regular cron will be invoked, in the last epoch of deadlines.
        let vest_begin = current_epoch + spec.initial_delay; // Nothing unlocks here, this is just the start of the clock.
        let vest_period = spec.vest_period;
        let mut vested_so_far = TokenAmount::zero();

        let mut epoch = vest_begin;

        while vested_so_far < *vesting_sum {
            epoch += spec.step_duration;

            let vest_epoch = QuantSpec {
                unit: spec.quantization,
                offset: proving_period_start,
            }
            .quantize_up(epoch);

            let elapsed = vest_epoch - vest_begin;
            let target_vest = if elapsed < spec.vest_period {
                // Linear vesting
                (vesting_sum * elapsed).div_floor(&TokenAmount::from(vest_period))
            } else {
                vesting_sum.clone()
            };

            let vest_this_time = &target_vest - vested_so_far;
            vested_so_far = target_vest;

            match epoch_to_index.get(&vest_epoch) {
                Some(&index) => {
                    // epoch already exists. Load existing entry and update amount.
                    self.funds[index].amount += vest_this_time;
                }
                None => {
                    // append a new entry, vec will be sorted by epoch later.
                    epoch_to_index.insert(vest_epoch, self.funds.len());
                    self.funds.push(VestingFund {
                        epoch: vest_epoch,
                        amount: vest_this_time,
                    });
                }
            }
        }

        self.funds.sort_by_key(|fund| fund.epoch);
    }

    pub fn unlock_unvested_funds(
        &mut self,
        current_epoch: ChainEpoch,
        target: &TokenAmount,
    ) -> TokenAmount {
        let mut amount_unlocked = TokenAmount::from(0);
        let mut last = None;
        let mut start = 0;
        for (i, vf) in self.funds.iter_mut().enumerate() {
            if &amount_unlocked >= target {
                break;
            }

            if vf.epoch >= current_epoch {
                let unlock_amount = std::cmp::min(target - &amount_unlocked, vf.amount.clone());
                amount_unlocked += &unlock_amount;
                let new_amount = &vf.amount - &unlock_amount;

                if new_amount.is_zero() {
                    last = Some(i);
                } else {
                    vf.amount = new_amount;
                }
            } else {
                start = i + 1;
            }
        }

        if let Some(end) = last {
            self.funds.drain(start..=end);
        }

        amount_unlocked
    }
}
