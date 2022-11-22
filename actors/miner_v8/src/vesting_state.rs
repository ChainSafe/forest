// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{iter, mem};

use fvm_ipld_encoding::tuple::*;
use fvm_shared::clock::{ChainEpoch, QuantSpec};
use fvm_shared::econ::TokenAmount;
use itertools::{EitherOrBoth, Itertools};
use num_traits::Zero;

use super::VestSpec;

// Represents miner funds that will vest at the given epoch.
#[derive(Debug, Serialize_tuple, Deserialize_tuple)]
pub struct VestingFund {
    pub epoch: ChainEpoch,
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
        // Quantization is aligned with when regular cron will be invoked, in the last epoch of deadlines.
        let vest_begin = current_epoch + spec.initial_delay; // Nothing unlocks here, this is just the start of the clock.
        let mut vested_so_far = TokenAmount::zero();

        let mut epoch = vest_begin;

        // Create an iterator for the vesting schedule we're going to "join" with the current
        // vesting schedule.
        let new_funds = iter::from_fn(|| {
            if vested_so_far >= *vesting_sum {
                return None;
            }

            epoch += spec.step_duration;

            let vest_epoch = QuantSpec {
                unit: spec.quantization,
                offset: proving_period_start,
            }
            .quantize_up(epoch);

            let elapsed = vest_epoch - vest_begin;
            let target_vest = if elapsed < spec.vest_period {
                // Linear vesting
                (vesting_sum * elapsed).div_floor(spec.vest_period)
            } else {
                vesting_sum.clone()
            };

            let vest_this_time = &target_vest - &vested_so_far;
            vested_so_far = target_vest;

            Some(VestingFund {
                epoch: vest_epoch,
                amount: vest_this_time,
            })
        });

        // Take the old funds array and replace it with a new one.
        let funds_len = self.funds.len();
        let old_funds = mem::replace(&mut self.funds, Vec::with_capacity(funds_len));

        // Fill back in the funds array, merging existing and new schedule.
        self.funds.extend(
            old_funds
                .into_iter()
                .merge_join_by(new_funds, |a, b| a.epoch.cmp(&b.epoch))
                .map(|item| match item {
                    EitherOrBoth::Left(a) => a,
                    EitherOrBoth::Right(b) => b,
                    EitherOrBoth::Both(a, b) => VestingFund {
                        epoch: a.epoch,
                        amount: a.amount + b.amount,
                    },
                }),
        );
    }

    pub fn unlock_unvested_funds(
        &mut self,
        current_epoch: ChainEpoch,
        target: &TokenAmount,
    ) -> TokenAmount {
        let mut amount_unlocked = TokenAmount::from_atto(0);
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
