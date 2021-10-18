// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{multihash::MultihashDigest, Cid, Code::Identity, RAW};

pub const SYSTEM_ACTOR_CODE_ID_NAME: &str = "fil/4/system";
pub const INIT_ACTOR_CODE_ID_NAME: &str = "fil/4/init";
pub const CRON_ACTOR_CODE_ID_NAME: &str = "fil/4/cron";
pub const ACCOUNT_ACTOR_CODE_ID_NAME: &str = "fil/4/account";
pub const POWER_ACTOR_CODE_ID_NAME: &str = "fil/4/storagepower";
pub const MINER_ACTOR_CODE_ID_NAME: &str = "fil/4/storageminer";
pub const MARKET_ACTOR_CODE_ID_NAME: &str = "fil/4/storagemarket";
pub const PAYCH_ACTOR_CODE_ID_NAME: &str = "fil/4/paymentchannel";
pub const MULTISIG_ACTOR_CODE_ID_NAME: &str = "fil/4/multisig";
pub const REWARD_ACTOR_CODE_ID_NAME: &str = "fil/4/reward";
pub const VERIFREG_ACTOR_CODE_ID_NAME: &str = "fil/4/verifiedregistry";
pub const CHAOS_ACTOR_CODE_ID_NAME: &str = "fil/4/chaos";

lazy_static! {
    pub static ref SYSTEM_ACTOR_CODE_ID: Cid = make_builtin(b"fil/5/system");
    pub static ref INIT_ACTOR_CODE_ID: Cid = make_builtin(b"fil/5/init");
    pub static ref CRON_ACTOR_CODE_ID: Cid = make_builtin(b"fil/5/cron");
    pub static ref ACCOUNT_ACTOR_CODE_ID: Cid = make_builtin(b"fil/5/account");
    pub static ref POWER_ACTOR_CODE_ID: Cid = make_builtin(b"fil/5/storagepower");
    pub static ref MINER_ACTOR_CODE_ID: Cid = make_builtin(b"fil/5/storageminer");
    pub static ref MARKET_ACTOR_CODE_ID: Cid = make_builtin(b"fil/5/storagemarket");
    pub static ref PAYCH_ACTOR_CODE_ID: Cid = make_builtin(b"fil/5/paymentchannel");
    pub static ref MULTISIG_ACTOR_CODE_ID: Cid = make_builtin(b"fil/5/multisig");
    pub static ref REWARD_ACTOR_CODE_ID: Cid = make_builtin(b"fil/5/reward");
    pub static ref VERIFREG_ACTOR_CODE_ID: Cid = make_builtin(b"fil/5/verifiedregistry");
    pub static ref CHAOS_ACTOR_CODE_ID: Cid = make_builtin(b"fil/5/chaos");

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

/// Given an actor code Cid, returns the name of the actor.
pub fn actor_name_by_code(code: &Cid) -> Result<&str, String> {
    match code {
        x if x == &*SYSTEM_ACTOR_CODE_ID => Ok(SYSTEM_ACTOR_CODE_ID_NAME),
        x if x == &*INIT_ACTOR_CODE_ID => Ok(INIT_ACTOR_CODE_ID_NAME),
        x if x == &*CRON_ACTOR_CODE_ID => Ok(CRON_ACTOR_CODE_ID_NAME),
        x if x == &*ACCOUNT_ACTOR_CODE_ID => Ok(ACCOUNT_ACTOR_CODE_ID_NAME),
        x if x == &*POWER_ACTOR_CODE_ID => Ok(POWER_ACTOR_CODE_ID_NAME),
        x if x == &*MINER_ACTOR_CODE_ID => Ok(MINER_ACTOR_CODE_ID_NAME),
        x if x == &*MARKET_ACTOR_CODE_ID => Ok(MARKET_ACTOR_CODE_ID_NAME),
        x if x == &*PAYCH_ACTOR_CODE_ID => Ok(PAYCH_ACTOR_CODE_ID_NAME),
        x if x == &*MULTISIG_ACTOR_CODE_ID => Ok(MULTISIG_ACTOR_CODE_ID_NAME),
        x if x == &*REWARD_ACTOR_CODE_ID => Ok(REWARD_ACTOR_CODE_ID_NAME),
        x if x == &*VERIFREG_ACTOR_CODE_ID => Ok(VERIFREG_ACTOR_CODE_ID_NAME),
        x if x == &*CHAOS_ACTOR_CODE_ID => Ok(CHAOS_ACTOR_CODE_ID_NAME),
        _ => Err(format!("{} is not a valid code", code)),
    }
}
