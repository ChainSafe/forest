// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use anyhow::Result;
use cid::Cid;

macro_rules! register_evm_version {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{ConstructorParams, Method};

        register_actor_methods!(
            $registry,
            $code_cid,
            [(Method::Constructor, ConstructorParams)]
        );
    }};
}

pub(crate) fn register_evm_actor_methods(registry: &mut MethodRegistry, cid: Cid) {
    register_evm_version!(registry, cid, fil_actor_evm_state::v10);
    register_evm_version!(registry, cid, fil_actor_evm_state::v11);
    register_evm_version!(registry, cid, fil_actor_evm_state::v12);
    register_evm_version!(registry, cid, fil_actor_evm_state::v13);
    register_evm_version!(registry, cid, fil_actor_evm_state::v14);
    register_evm_version!(registry, cid, fil_actor_evm_state::v15);
    register_evm_version!(registry, cid, fil_actor_evm_state::v16);
}
