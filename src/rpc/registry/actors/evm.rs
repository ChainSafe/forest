// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::eth::types::GetStorageAtParams;
use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use cid::Cid;
use fil_actors_shared::actor_versions::ActorVersion;
use fvm_ipld_encoding::RawBytes;

macro_rules! register_evm_version {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{ConstructorParams, DelegateCallParams, Method};

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, ConstructorParams),
                (Method::Resurrect, ConstructorParams),
                (Method::InvokeContract, RawBytes),
                (Method::InvokeContractDelegate, DelegateCallParams),
            ]
        );

        $registry.register_method(
            $code_cid,
            Method::GetStorageAt as MethodNum,
            GetStorageAtParams::deserialize_params,
        );

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::GetBytecode, empty),
                (Method::GetBytecodeHash, empty)
            ]
        );
    }};
}

pub(crate) fn register_evm_actor_methods(
    registry: &mut MethodRegistry,
    cid: Cid,
    version: ActorVersion,
) {
    match version {
        ActorVersion::V8 | ActorVersion::V9 => {}
        ActorVersion::V10 => register_evm_version!(registry, cid, fil_actor_evm_state::v10),
        ActorVersion::V11 => register_evm_version!(registry, cid, fil_actor_evm_state::v11),
        ActorVersion::V12 => register_evm_version!(registry, cid, fil_actor_evm_state::v12),
        ActorVersion::V13 => register_evm_version!(registry, cid, fil_actor_evm_state::v13),
        ActorVersion::V14 => register_evm_version!(registry, cid, fil_actor_evm_state::v14),
        ActorVersion::V15 => register_evm_version!(registry, cid, fil_actor_evm_state::v15),
        ActorVersion::V16 => register_evm_version!(registry, cid, fil_actor_evm_state::v16),
    }
}
