// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::VestSpec;
use clock::ChainEpoch;
use encoding::tuple::*;
use fil_types::deadlines::QuantSpec;
use num_bigint::{bigint_ser, Integer};
use num_traits::Zero;
use std::{cmp::Ordering, collections::HashMap};
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
        spec: VestSpec,
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
                // Linear vesting, PARAM_FINISH
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
        // retain funds that should have vested
        // TODO: this could also benefit from binary search
        let start = self
            .funds
            .iter()
            .position(|fund| fund.epoch >= current_epoch)
            .unwrap_or(self.funds.len());

        // we keep track of the remaining funds and error out with the upper bound of the unlocked
        // funds when the target is reached
        // note: `try_fold` continues iteration when `Ok` is returned and stops when `Err` is returned
        let remaining_or_end = self.funds.iter_mut().enumerate().skip(start).try_fold(
            target.clone(),
            |remaining, (i, fund)| {
                match fund.amount.cmp(&remaining) {
                    Ordering::Less => {
                        // this fund is unlocked
                        Ok(remaining - &fund.amount)
                    }
                    Ordering::Equal => {
                        // this fund is the last one to be unlocked
                        Err(i + 1)
                    }
                    Ordering::Greater => {
                        // the amount of this fund is decreased, the previous fund was the last one to be fully unlocked
                        fund.amount -= remaining;
                        Err(i)
                    }
                }
            },
        );

        let amount_unlocked = match remaining_or_end {
            Ok(remaining) => {
                // the target wasn't reached, all unvested funds are unlocked
                self.funds.drain(start..);
                target - remaining
            }
            Err(end) => {
                // the target was reached so it is exactly the unlocked amount
                self.funds.drain(start..end);
                target.clone()
            }
        };

        amount_unlocked
    }
}
