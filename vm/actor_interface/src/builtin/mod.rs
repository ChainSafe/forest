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

use crate::ActorVersion;

use cid::Cid;

use fil_actors_runtime_v8::builtin::network;
use fil_actors_runtime_v8::builtin::singletons;

pub use fil_actor_reward_v8::AwardBlockRewardParams;

pub use fvm_shared::clock::EPOCH_DURATION_SECONDS;
pub use fvm_shared::smooth::FilterEstimate;
pub const EPOCHS_IN_DAY: fvm_shared::clock::ChainEpoch = network::EPOCHS_IN_DAY;

// Aliases for common addresses
pub static CHAOS_ACTOR_ADDR: &singletons::CHAOS_ACTOR_ADDR = &singletons::CHAOS_ACTOR_ADDR;
pub static BURNT_FUNDS_ACTOR_ADDR: &singletons::BURNT_FUNDS_ACTOR_ADDR =
    &singletons::BURNT_FUNDS_ACTOR_ADDR;

pub static RESERVE_ADDRESS: address::Address = address::Address::new_id(90);

#[macro_export]
macro_rules! load_actor_state {
    ($store:expr, $actor:expr, $id:ident) => {
        if $actor.code == *actorv6::$id {
            use anyhow::Context;
            $store
                .get_obj(&$actor.state)?
                .context("Actor state doesn't exist in store")
                .map(State::V6)
        } else if $actor.code == *actorv5::$id {
            use anyhow::Context;
            $store
                .get_obj(&$actor.state)?
                .context("Actor state doesn't exist in store")
                .map(State::V5)
        } else if $actor.code == *actorv4::$id {
            use anyhow::Context;
            $store
                .get_obj(&$actor.state)?
                .context("Actor state doesn't exist in store")
                .map(State::V4)
        } else if $actor.code == *actorv3::$id {
            use anyhow::Context;
            $store
                .get_obj(&$actor.state)?
                .context("Actor state doesn't exist in store")
                .map(State::V3)
        } else if $actor.code == *actorv2::$id {
            use anyhow::Context;
            $store
                .get_obj(&$actor.state)?
                .context("Actor state doesn't exist in store")
                .map(State::V2)
        } else if $actor.code == *actorv0::$id {
            use anyhow::Context;
            $store
                .get_obj(&$actor.state)?
                .context("Actor state doesn't exist in store")
                .map(State::V0)
        } else {
            Err(anyhow::anyhow!("Unknown actor code {}", $actor.code))
        }
    };
}

/// Returns true if the code belongs to a builtin actor.
pub fn is_builtin_actor(_code: &Cid) -> bool {
    unimplemented!()
}

/// Returns true if the code belongs to an account actor.
pub fn is_account_actor(code: &Cid) -> bool {
    account::is_v8_account_cid(code)
}

/// Returns true if the code belongs to a singleton actor.
pub fn is_singleton_actor(_code: &Cid) -> bool {
    unimplemented!()
}

/// Returns true if the code belongs to a miner actor.
pub fn is_miner_actor(_code: &Cid) -> bool {
    unimplemented!()
}

/// Returns an actor's version or None if it was not a builtin
pub fn actor_version(_code: &Cid) -> Option<ActorVersion> {
    None
}
