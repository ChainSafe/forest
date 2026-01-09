// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use cid::Cid;
use fil_actors_shared::actor_versions::ActorVersion;

macro_rules! register_eth_account_reg_version {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::*;

        // Constructor has no parameters
        register_actor_methods!($registry, $code_cid, [(Method::Constructor, empty),]);
    }};
}

pub(crate) fn register_actor_methods(
    registry: &mut MethodRegistry,
    cid: Cid,
    version: ActorVersion,
) {
    match version {
        ActorVersion::V8 | ActorVersion::V9 => {}
        ActorVersion::V10 => {
            register_eth_account_reg_version!(registry, cid, fil_actor_ethaccount_state::v10)
        }
        ActorVersion::V11 => {
            register_eth_account_reg_version!(registry, cid, fil_actor_ethaccount_state::v11)
        }
        ActorVersion::V12 => {
            register_eth_account_reg_version!(registry, cid, fil_actor_ethaccount_state::v12)
        }
        ActorVersion::V13 => {
            register_eth_account_reg_version!(registry, cid, fil_actor_ethaccount_state::v13)
        }
        ActorVersion::V14 => {
            register_eth_account_reg_version!(registry, cid, fil_actor_ethaccount_state::v14)
        }
        ActorVersion::V15 => {
            register_eth_account_reg_version!(registry, cid, fil_actor_ethaccount_state::v15)
        }
        ActorVersion::V16 => {
            register_eth_account_reg_version!(registry, cid, fil_actor_ethaccount_state::v16)
        }
        ActorVersion::V17 => {
            register_eth_account_reg_version!(registry, cid, fil_actor_ethaccount_state::v17)
        }
    }
}
