// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use anyhow::Result;
use cid::Cid;

macro_rules! register_miner_version {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{ChangeWorkerAddressParams, Method, MinerConstructorParams};

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, MinerConstructorParams),
                (Method::ChangeWorkerAddress, ChangeWorkerAddressParams)
            ]
        );
    }};
}

pub(crate) fn register_miner_actor_methods(registry: &mut MethodRegistry, cid: Cid) {
    register_miner_version!(registry, cid, fil_actor_miner_state::v8);
    register_miner_version!(registry, cid, fil_actor_miner_state::v9);
    register_miner_version!(registry, cid, fil_actor_miner_state::v10);
    register_miner_version!(registry, cid, fil_actor_miner_state::v11);
    register_miner_version!(registry, cid, fil_actor_miner_state::v12);
    register_miner_version!(registry, cid, fil_actor_miner_state::v13);
    register_miner_version!(registry, cid, fil_actor_miner_state::v14);
    register_miner_version!(registry, cid, fil_actor_miner_state::v15);
    register_miner_version!(registry, cid, fil_actor_miner_state::v16);
}
