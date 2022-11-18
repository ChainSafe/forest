// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared::address::Address;
use fvm_shared::ActorID;

use paste::paste;

macro_rules! define_singletons {
    ($($name:ident = $id:literal,)*) => {
        $(
            paste! {
                pub const [<$name _ID>]: ActorID = $id;
                pub const [<$name _ADDR>]: Address = Address::new_id([<$name _ID>]);
            }
        )*
    }
}

define_singletons! {
    SYSTEM_ACTOR = 0,
    INIT_ACTOR = 1,
    REWARD_ACTOR = 2,
    CRON_ACTOR = 3,
    STORAGE_POWER_ACTOR = 4,
    STORAGE_MARKET_ACTOR = 5,
    VERIFIED_REGISTRY_ACTOR = 6,
    DATACAP_TOKEN_ACTOR = 7,
    CHAOS_ACTOR = 98,
    BURNT_FUNDS_ACTOR = 99,
}

/// Defines first available ID address after builtin actors
pub const FIRST_NON_SINGLETON_ADDR: ActorID = 100;
