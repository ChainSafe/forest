// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use cid::Cid;

macro_rules! register_eth_account_reg_version {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::*;

        // Constructor has no parameters
        register_actor_methods!($registry, $code_cid, [(Method::Constructor, empty),]);
    }};
}

pub(crate) fn register_actor_methods(registry: &mut MethodRegistry, cid: Cid, version: u64) {
    match version {
        10 => register_eth_account_reg_version!(registry, cid, fil_actor_ethaccount_state::v10),
        11 => register_eth_account_reg_version!(registry, cid, fil_actor_ethaccount_state::v11),
        12 => register_eth_account_reg_version!(registry, cid, fil_actor_ethaccount_state::v12),
        13 => register_eth_account_reg_version!(registry, cid, fil_actor_ethaccount_state::v13),
        14 => register_eth_account_reg_version!(registry, cid, fil_actor_ethaccount_state::v14),
        15 => register_eth_account_reg_version!(registry, cid, fil_actor_ethaccount_state::v15),
        16 => register_eth_account_reg_version!(registry, cid, fil_actor_ethaccount_state::v16),
        _ => {}
    }
}