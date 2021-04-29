// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{multihash::MultihashDigest, Cid, Code::Identity, RAW};

lazy_static! {
    pub static ref SYSTEM_ACTOR_CODE_ID: Cid = make_builtin(b"fil/4/system");
    pub static ref INIT_ACTOR_CODE_ID: Cid = make_builtin(b"fil/4/init");
    pub static ref CRON_ACTOR_CODE_ID: Cid = make_builtin(b"fil/4/cron");
    pub static ref ACCOUNT_ACTOR_CODE_ID: Cid = make_builtin(b"fil/4/account");
    pub static ref POWER_ACTOR_CODE_ID: Cid = make_builtin(b"fil/4/storagepower");
    pub static ref MINER_ACTOR_CODE_ID: Cid = make_builtin(b"fil/4/storageminer");
    pub static ref MARKET_ACTOR_CODE_ID: Cid = make_builtin(b"fil/4/storagemarket");
    pub static ref PAYCH_ACTOR_CODE_ID: Cid = make_builtin(b"fil/4/paymentchannel");
    pub static ref MULTISIG_ACTOR_CODE_ID: Cid = make_builtin(b"fil/4/multisig");
    pub static ref REWARD_ACTOR_CODE_ID: Cid = make_builtin(b"fil/4/reward");
    pub static ref VERIFREG_ACTOR_CODE_ID: Cid = make_builtin(b"fil/4/verifiedregistry");
    pub static ref CHAOS_ACTOR_CODE_ID: Cid = make_builtin(b"fil/4/chaos");

    /// Set of actor code types that can represent external signing parties.
    pub static ref CALLER_TYPES_SIGNABLE: [Cid; 2] =
        [*ACCOUNT_ACTOR_CODE_ID, *MULTISIG_ACTOR_CODE_ID];
}

fn make_builtin(bz: &[u8]) -> Cid {
    Cid::new_v1(RAW, Identity.digest(bz))
}

/// Returns true if the code `Cid` belongs to a builtin actor.
pub fn is_builtin_actor(code: &Cid) -> bool {
    code == &*SYSTEM_ACTOR_CODE_ID
        || code == &*INIT_ACTOR_CODE_ID
        || code == &*CRON_ACTOR_CODE_ID
        || code == &*ACCOUNT_ACTOR_CODE_ID
        || code == &*POWER_ACTOR_CODE_ID
        || code == &*MINER_ACTOR_CODE_ID
        || code == &*MARKET_ACTOR_CODE_ID
        || code == &*PAYCH_ACTOR_CODE_ID
        || code == &*MULTISIG_ACTOR_CODE_ID
        || code == &*REWARD_ACTOR_CODE_ID
        || code == &*VERIFREG_ACTOR_CODE_ID
}

/// Returns true if the code belongs to a singleton actor.
pub fn is_singleton_actor(code: &Cid) -> bool {
    code == &*SYSTEM_ACTOR_CODE_ID
        || code == &*INIT_ACTOR_CODE_ID
        || code == &*REWARD_ACTOR_CODE_ID
        || code == &*CRON_ACTOR_CODE_ID
        || code == &*POWER_ACTOR_CODE_ID
        || code == &*MARKET_ACTOR_CODE_ID
        || code == &*VERIFREG_ACTOR_CODE_ID
}

/// Returns true if the code belongs to an account actor.
pub fn is_account_actor(code: &Cid) -> bool {
    code == &*ACCOUNT_ACTOR_CODE_ID
}

/// Tests whether a code CID represents an actor that can be an external principal: i.e. an account or multisig.
pub fn is_principal(code: &Cid) -> bool {
    CALLER_TYPES_SIGNABLE.iter().any(|c| c == code)
}
