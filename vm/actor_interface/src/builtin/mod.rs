// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod account;
pub mod cron;
pub mod init;
pub mod market;
pub mod miner;
pub mod multisig;
pub mod power;
pub mod reward;
pub mod system;

use cid::Cid;
use num_bigint::BigInt;

pub const EPOCH_DURATION_SECONDS: clock::ChainEpoch = actorv0::EPOCH_DURATION_SECONDS;
pub const EPOCHS_IN_DAY: clock::ChainEpoch = actorv0::EPOCHS_IN_DAY;

// Aliases for common addresses
pub static CHAOS_ACTOR_ADDR: &actorv0::CHAOS_ACTOR_ADDR = &actorv0::CHAOS_ACTOR_ADDR;
pub static BURNT_FUNDS_ACTOR_ADDR: &actorv0::BURNT_FUNDS_ACTOR_ADDR =
    &actorv0::BURNT_FUNDS_ACTOR_ADDR;
pub static RESERVE_ADDRESS: &actorv0::RESERVE_ADDRESS = &actorv0::RESERVE_ADDRESS;

/// Returns true if the code belongs to a builtin actor.
pub fn is_builtin_actor(code: &Cid) -> bool {
    actorv0::is_builtin_actor(code)
        || actorv2::is_builtin_actor(code)
        || actorv3::is_builtin_actor(code)
        || actorv4::is_builtin_actor(code)
        || actorv5::is_builtin_actor(code)
}

/// Returns true if the code belongs to an account actor.
pub fn is_account_actor(code: &Cid) -> bool {
    actorv0::is_account_actor(code)
        || actorv2::is_account_actor(code)
        || actorv3::is_account_actor(code)
        || actorv4::is_account_actor(code)
        || actorv5::is_account_actor(code)
}

/// Returns true if the code belongs to a singleton actor.
pub fn is_singleton_actor(code: &Cid) -> bool {
    actorv0::is_singleton_actor(code)
        || actorv2::is_singleton_actor(code)
        || actorv3::is_singleton_actor(code)
        || actorv4::is_singleton_actor(code)
        || actorv5::is_singleton_actor(code)
}

/// Returns true if the code belongs to a miner actor.
pub fn is_miner_actor(code: &Cid) -> bool {
    code == &*actorv0::MINER_ACTOR_CODE_ID
        || code == &*actorv2::MINER_ACTOR_CODE_ID
        || code == &*actorv3::MINER_ACTOR_CODE_ID
        || code == &*actorv4::MINER_ACTOR_CODE_ID
        || code == &*actorv5::MINER_ACTOR_CODE_ID
}

#[derive(Default, Clone, Debug, PartialEq)]
pub struct FilterEstimate {
    pub position: BigInt,
    pub velocity: BigInt,
}

impl From<actorv0::util::smooth::FilterEstimate> for FilterEstimate {
    fn from(filter_est: actorv0::util::smooth::FilterEstimate) -> Self {
        Self {
            position: filter_est.position,
            velocity: filter_est.velocity,
        }
    }
}

impl From<actorv2::util::smooth::FilterEstimate> for FilterEstimate {
    fn from(filter_est: actorv2::util::smooth::FilterEstimate) -> Self {
        Self {
            position: filter_est.position,
            velocity: filter_est.velocity,
        }
    }
}

impl From<actorv3::util::smooth::FilterEstimate> for FilterEstimate {
    fn from(filter_est: actorv3::util::smooth::FilterEstimate) -> Self {
        Self {
            position: filter_est.position,
            velocity: filter_est.velocity,
        }
    }
}

impl From<actorv4::util::smooth::FilterEstimate> for FilterEstimate {
    fn from(filter_est: actorv4::util::smooth::FilterEstimate) -> Self {
        Self {
            position: filter_est.position,
            velocity: filter_est.velocity,
        }
    }
}
impl From<actorv5::util::smooth::FilterEstimate> for FilterEstimate {
    fn from(filter_est: actorv5::util::smooth::FilterEstimate) -> Self {
        Self {
            position: filter_est.position,
            velocity: filter_est.velocity,
        }
    }
}
