// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use anyhow::Result;
use cid::Cid;

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

pub(crate) fn register_actor_methods(registry: &mut MethodRegistry, cid: Cid, version: u64) {
    match version {
        8 => register_multisig_v8!(registry, cid, fil_actor_multisig_state::v8),
        9 => register_multisig_v9_plus!(registry, cid, fil_actor_multisig_state::v9),
        10 => register_multisig_v9_plus!(registry, cid, fil_actor_multisig_state::v10),
        11 => register_multisig_v9_plus!(registry, cid, fil_actor_multisig_state::v11),
        12 => register_multisig_v9_plus!(registry, cid, fil_actor_multisig_state::v12),
        13 => register_multisig_v9_plus!(registry, cid, fil_actor_multisig_state::v13),
        14 => register_multisig_v9_plus!(registry, cid, fil_actor_multisig_state::v14),
        15 => register_multisig_v9_plus!(registry, cid, fil_actor_multisig_state::v15),
        16 => register_multisig_v9_plus!(registry, cid, fil_actor_multisig_state::v16),
        _ => {}
    }
}
