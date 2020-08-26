// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{multihash::Identity, Cid, Codec};

lazy_static! {
    pub static ref SYSTEM_ACTOR_CODE_ID: Cid = make_builtin(b"fil/1/system");
    pub static ref INIT_ACTOR_CODE_ID: Cid = make_builtin(b"fil/1/init");
    pub static ref CRON_ACTOR_CODE_ID: Cid = make_builtin(b"fil/1/cron");
    pub static ref ACCOUNT_ACTOR_CODE_ID: Cid = make_builtin(b"fil/1/account");
    pub static ref POWER_ACTOR_CODE_ID: Cid = make_builtin(b"fil/1/storagepower");
    pub static ref MINER_ACTOR_CODE_ID: Cid = make_builtin(b"fil/1/storageminer");
    pub static ref MARKET_ACTOR_CODE_ID: Cid = make_builtin(b"fil/1/storagemarket");
    pub static ref PAYCH_ACTOR_CODE_ID: Cid = make_builtin(b"fil/1/paymentchannel");
    pub static ref MULTISIG_ACTOR_CODE_ID: Cid = make_builtin(b"fil/1/multisig");
    pub static ref REWARD_ACTOR_CODE_ID: Cid = make_builtin(b"fil/1/reward");
    pub static ref VERIFREG_ACTOR_CODE_ID: Cid = make_builtin(b"fil/1/verifiedregistry");
    pub static ref PUPPET_ACTOR_CODE_ID : Cid = make_builtin(b"fil/1/puppet");

    // Set of actor code types that can represent external signing parties.
    pub static ref CALLER_TYPES_SIGNABLE: [Cid; 2] =
        [ACCOUNT_ACTOR_CODE_ID.clone(), MULTISIG_ACTOR_CODE_ID.clone()];
}

fn make_builtin(bz: &[u8]) -> Cid {
    Cid::new_v1(Codec::Raw, Identity::digest(bz))
}

// Tests whether a code CID represents an actor that can be an external principal: i.e. an account or multisig.
// We could do something more sophisticated here: https://github.com/filecoin-project/specs-actors/issues/178
pub fn is_principal(code: &Cid) -> bool {
    CALLER_TYPES_SIGNABLE.iter().any(|c| c == code)
}
