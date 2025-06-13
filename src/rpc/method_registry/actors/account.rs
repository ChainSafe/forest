// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::method_registry::registry::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use anyhow::Result;
use cid::Cid;

/// Macro to generate account method registration for different versions
macro_rules! register_account_version {
    // For versions that use types module (v15, v16)
    ($registry:expr, $code_cid:expr, $state_version:path, with_types) => {{
        use $state_version::{Method, types};

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, types::ConstructorParams),
                (
                    Method::AuthenticateMessageExported,
                    types::AuthenticateMessageParams
                )
            ]
        );
    }};

    // For versions that don't use types module (v11-v14)
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{AuthenticateMessageParams, ConstructorParams, Method};

        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, ConstructorParams),
                (
                    Method::AuthenticateMessageExported,
                    AuthenticateMessageParams
                )
            ]
        );
    }};
}

// register account actor methods, cid is unique for each version of the actor
pub(crate) fn register_account_actor_methods(registry: &mut MethodRegistry, cid: Cid) {
    register_account_version!(registry, cid, fil_actor_account_state::v11);
    register_account_version!(registry, cid, fil_actor_account_state::v12);
    register_account_version!(registry, cid, fil_actor_account_state::v13);
    register_account_version!(registry, cid, fil_actor_account_state::v14);
    register_account_version!(registry, cid, fil_actor_account_state::v15, with_types);
    register_account_version!(registry, cid, fil_actor_account_state::v16, with_types);
}
