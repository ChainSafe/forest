// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::method_registry::registry::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use anyhow::Result;
use cid::Cid;

pub(crate) fn register_evm_actor_methods(registry: &mut MethodRegistry, cid: Cid) {
    register_evm_v15_methods(registry, cid);
    register_evm_v16_methods(registry, cid);
}

fn register_evm_v15_methods(registry: &mut MethodRegistry, code_cid: Cid) {
    use fil_actor_evm_state::v15::{ConstructorParams, Method};

    register_actor_methods!(
        registry,
        code_cid,
        [(Method::Constructor, ConstructorParams)]
    );
}

fn register_evm_v16_methods(registry: &mut MethodRegistry, code_cid: Cid) {
    use fil_actor_evm_state::v16::{ConstructorParams, Method};

    register_actor_methods!(
        registry,
        code_cid,
        [(Method::Constructor, ConstructorParams)]
    );
}
