// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use fil_types::ActorID;

lazy_static! {
    pub static ref SYSTEM_ACTOR_ADDR: Address         = Address::new_id(0);
    pub static ref INIT_ACTOR_ADDR: Address           = Address::new_id(1);
    pub static ref REWARD_ACTOR_ADDR: Address         = Address::new_id(2);
    pub static ref CRON_ACTOR_ADDR: Address           = Address::new_id(3);
    pub static ref STORAGE_POWER_ACTOR_ADDR: Address  = Address::new_id(4);
    pub static ref STORAGE_MARKET_ACTOR_ADDR: Address = Address::new_id(5);
    pub static ref VERIFIED_REGISTRY_ACTOR_ADDR: Address = Address::new_id(6);

    pub static ref CHAOS_ACTOR_ADDR: Address    = Address::new_id(98);

    /// Distinguished AccountActor that is the destination of all burnt funds.
    pub static ref BURNT_FUNDS_ACTOR_ADDR: Address = Address::new_id(99);

    pub static ref RESERVE_ADDRESS: Address = Address::new_id(90);
}

/// Defines first available ID address after builtin actors
pub const FIRST_NON_SINGLETON_ADDR: ActorID = 100;
