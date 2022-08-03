// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
mod consensus;
mod proposer;
mod validation;

use std::sync::Arc;

pub use consensus::{DelegatedConsensus, DelegatedConsensusError};
pub use proposer::DelegatedProposer;

pub fn reward_calc() -> Arc<dyn interpreter::RewardCalc> {
    Arc::new(interpreter::NoRewardCalc)
}
