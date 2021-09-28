// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod quantize;

pub use self::quantize::*;
use clock::ChainEpoch;
use serde::{Deserialize, Serialize};

/// Deadline calculations with respect to a current epoch.
/// "Deadline" refers to the window during which proofs may be submitted.
/// Windows are non-overlapping ranges [Open, Close), but the challenge epoch for a window occurs
/// before the window opens.
#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Copy, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct DeadlineInfo {
    /// Epoch at which this info was calculated.
    pub current_epoch: ChainEpoch,
    /// First epoch of the proving period (<= CurrentEpoch).
    pub period_start: ChainEpoch,
    /// Current deadline index, in [0..WPoStProvingPeriodDeadlines).
    pub index: u64,
    /// First epoch from which a proof may be submitted (>= CurrentEpoch).
    pub open: ChainEpoch,
    /// First epoch from which a proof may no longer be submitted (>= Open).
    pub close: ChainEpoch,
    /// Epoch at which to sample the chain for challenge (< Open).
    pub challenge: ChainEpoch,
    /// First epoch at which a fault declaration is rejected (< Open).
    pub fault_cutoff: ChainEpoch,

    // Protocol parameters (This is intentionally included in the JSON response for deadlines)
    #[serde(rename = "WPoStPeriodDeadlines")]
    w_post_period_deadlines: u64,
    #[serde(rename = "WPoStProvingPeriod")]
    w_post_proving_period: ChainEpoch,
    #[serde(rename = "WPoStChallengeWindow")]
    w_post_challenge_window: ChainEpoch,
    #[serde(rename = "WPoStChallengeLookback")]
    w_post_challenge_lookback: ChainEpoch,
    fault_declaration_cutoff: ChainEpoch,
}

impl DeadlineInfo {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        period_start: ChainEpoch,
        deadline_idx: u64,
        current_epoch: ChainEpoch,
        w_post_period_deadlines: u64,
        w_post_proving_period: ChainEpoch,
        w_post_challenge_window: ChainEpoch,
        w_post_challenge_lookback: ChainEpoch,
        fault_declaration_cutoff: ChainEpoch,
    ) -> Self {
        if deadline_idx < w_post_period_deadlines {
            let deadline_open = period_start + (deadline_idx as i64 * w_post_challenge_window);
            Self {
                current_epoch,
                period_start,
                index: deadline_idx,
                open: deadline_open,
                close: deadline_open + w_post_challenge_window,
                challenge: deadline_open - w_post_challenge_lookback,
                fault_cutoff: deadline_open - fault_declaration_cutoff,
                w_post_period_deadlines,
                w_post_proving_period,
                w_post_challenge_window,
                w_post_challenge_lookback,
                fault_declaration_cutoff,
            }
        } else {
            let after_last_deadline = period_start + w_post_proving_period;
            Self {
                current_epoch,
                period_start,
                index: deadline_idx,
                open: after_last_deadline,
                close: after_last_deadline,
                challenge: after_last_deadline,
                fault_cutoff: 0,
                w_post_period_deadlines,
                w_post_proving_period,
                w_post_challenge_window,
                w_post_challenge_lookback,
                fault_declaration_cutoff,
            }
        }
    }

    /// Whether the proving period has begun.
    pub fn period_started(&self) -> bool {
        self.current_epoch >= self.period_start
    }

    /// Whether the proving period has elapsed.
    pub fn period_elapsed(&self) -> bool {
        self.current_epoch >= self.next_period_start()
    }

    /// The last epoch in the proving period.
    pub fn period_end(&self) -> ChainEpoch {
        self.period_start + self.w_post_proving_period - 1
    }

    /// The first epoch in the next proving period.
    pub fn next_period_start(&self) -> ChainEpoch {
        self.period_start + self.w_post_proving_period
    }

    /// Whether the current deadline is currently open.
    pub fn is_open(&self) -> bool {
        self.current_epoch >= self.open && self.current_epoch < self.close
    }

    /// Whether the current deadline has already closed.
    pub fn has_elapsed(&self) -> bool {
        self.current_epoch >= self.close
    }

    /// The last epoch during which a proof may be submitted.
    pub fn last(&self) -> ChainEpoch {
        self.close - 1
    }

    /// Epoch at which the subsequent deadline opens.
    pub fn next_open(&self) -> ChainEpoch {
        self.close
    }

    /// Whether the deadline's fault cutoff has passed.
    pub fn fault_cutoff_passed(&self) -> bool {
        self.current_epoch >= self.fault_cutoff
    }

    /// Returns the next instance of this deadline that has not yet elapsed.
    pub fn next_not_elapsed(self) -> Self {
        std::iter::successors(Some(self), |info| {
            Some(Self::new(
                info.next_period_start(),
                info.index,
                info.current_epoch,
                self.w_post_period_deadlines,
                self.w_post_proving_period,
                self.w_post_challenge_window,
                self.w_post_challenge_lookback,
                self.fault_declaration_cutoff,
            ))
        })
        .find(|info| !info.has_elapsed())
        .unwrap() // the iterator is infinite, so `find` won't ever return `None`
    }

    pub fn quant_spec(&self) -> QuantSpec {
        QuantSpec {
            unit: self.w_post_proving_period,
            offset: self.last(),
        }
    }
}
