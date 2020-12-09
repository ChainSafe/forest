// Copyright 2020 ChainSafe Systems
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

pub const EPOCH_DURATION_SECONDS: clock::ChainEpoch = actorv0::EPOCH_DURATION_SECONDS;
pub const EPOCHS_IN_DAY: clock::ChainEpoch = actorv0::EPOCHS_IN_DAY;

// Aliases for common addresses
pub static CHAOS_ACTOR_ADDR: &actorv0::CHAOS_ACTOR_ADDR = &actorv0::CHAOS_ACTOR_ADDR;
pub static BURNT_FUNDS_ACTOR_ADDR: &actorv0::BURNT_FUNDS_ACTOR_ADDR =
    &actorv0::BURNT_FUNDS_ACTOR_ADDR;
pub static RESERVE_ADDRESS: &actorv0::RESERVE_ADDRESS = &actorv0::RESERVE_ADDRESS;

/// Returns true if the code belongs to a builtin actor.
pub fn is_builtin_actor(code: &Cid) -> bool {
    actorv0::is_builtin_actor(code) || actorv2::is_builtin_actor(code)
}

/// Returns true if the code belongs to an account actor.
pub fn is_account_actor(code: &Cid) -> bool {
    actorv0::is_account_actor(code) || actorv2::is_account_actor(code)
}

/// Returns true if the code belongs to a singleton actor.
pub fn is_singleton_actor(code: &Cid) -> bool {
    actorv0::is_singleton_actor(code) || actorv2::is_singleton_actor(code)
}
