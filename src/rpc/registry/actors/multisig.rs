// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use cid::Cid;
use fil_actors_shared::actor_versions::ActorVersion;

// Macro for version 8 that doesn't have UniversalReceiverHook
macro_rules! register_multisig_v8 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            AddSignerParams, ChangeNumApprovalsThresholdParams, ConstructorParams,
            LockBalanceParams, Method, ProposeParams, RemoveSignerParams, SwapSignerParams,
            TxnIDParams,
        };

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, ConstructorParams),
                (Method::Propose, ProposeParams),
                (Method::Approve, TxnIDParams),
                (Method::Cancel, TxnIDParams),
                (Method::AddSigner, AddSignerParams),
                (Method::RemoveSigner, RemoveSignerParams),
                (Method::SwapSigner, SwapSignerParams),
                (
                    Method::ChangeNumApprovalsThreshold,
                    ChangeNumApprovalsThresholdParams
                ),
                (Method::LockBalance, LockBalanceParams),
            ]
        );
    }};
}

// Macro for versions 9+ that has UniversalReceiverHook
macro_rules! register_multisig_v9_plus {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            AddSignerParams, ChangeNumApprovalsThresholdParams, ConstructorParams,
            LockBalanceParams, Method, ProposeParams, RemoveSignerParams, SwapSignerParams,
            TxnIDParams,
        };

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, ConstructorParams),
                (Method::Propose, ProposeParams),
                (Method::Approve, TxnIDParams),
                (Method::Cancel, TxnIDParams),
                (Method::AddSigner, AddSignerParams),
                (Method::RemoveSigner, RemoveSignerParams),
                (Method::SwapSigner, SwapSignerParams),
                (
                    Method::ChangeNumApprovalsThreshold,
                    ChangeNumApprovalsThresholdParams
                ),
                (Method::LockBalance, LockBalanceParams),
            ]
        );

        // UniversalReceiverHook doesn't have specific params
        register_actor_methods!(
            $registry,
            $code_cid,
            [(Method::UniversalReceiverHook, empty)]
        );
    }};
}

pub(crate) fn register_actor_methods(
    registry: &mut MethodRegistry,
    cid: Cid,
    version: ActorVersion,
) {
    match version {
        ActorVersion::V8 => register_multisig_v8!(registry, cid, fil_actor_multisig_state::v8),
        ActorVersion::V9 => register_multisig_v9_plus!(registry, cid, fil_actor_multisig_state::v9),
        ActorVersion::V10 => {
            register_multisig_v9_plus!(registry, cid, fil_actor_multisig_state::v10)
        }
        ActorVersion::V11 => {
            register_multisig_v9_plus!(registry, cid, fil_actor_multisig_state::v11)
        }
        ActorVersion::V12 => {
            register_multisig_v9_plus!(registry, cid, fil_actor_multisig_state::v12)
        }
        ActorVersion::V13 => {
            register_multisig_v9_plus!(registry, cid, fil_actor_multisig_state::v13)
        }
        ActorVersion::V14 => {
            register_multisig_v9_plus!(registry, cid, fil_actor_multisig_state::v14)
        }
        ActorVersion::V15 => {
            register_multisig_v9_plus!(registry, cid, fil_actor_multisig_state::v15)
        }
        ActorVersion::V16 => {
            register_multisig_v9_plus!(registry, cid, fil_actor_multisig_state::v16)
        }
        ActorVersion::V17 => {
            register_multisig_v9_plus!(registry, cid, fil_actor_multisig_state::v17)
        }
    }
}
