// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::address::Address;
use crate::shim::message::MethodNum;
use cid::Cid;
use fil_actors_shared::actor_versions::ActorVersion;

fn register_account_version_v8(registry: &mut MethodRegistry, cid: Cid) {
    use fil_actor_account_state::v8::Method;

    register_actor_methods!(registry, cid, [(Method::Constructor, Address),]);
    register_actor_methods!(registry, cid, [(Method::PubkeyAddress, empty)]);
}

fn register_account_version_v9(registry: &mut MethodRegistry, cid: Cid) {
    use fil_actor_account_state::v9::{AuthenticateMessageParams, Method};
    register_actor_methods!(
        registry,
        cid,
        [
            (Method::Constructor, Address),
            (Method::AuthenticateMessage, AuthenticateMessageParams)
        ]
    );

    register_actor_methods!(
        registry,
        cid,
        [
            (Method::PubkeyAddress, empty),
            (Method::UniversalReceiverHook, empty)
        ]
    );
}

fn register_account_version_10(registry: &mut MethodRegistry, cid: Cid) {
    use fil_actor_account_state::v10::{AuthenticateMessageParams, Method};

    register_actor_methods!(
        registry,
        cid,
        [
            (Method::Constructor, Address),
            (
                Method::AuthenticateMessageExported,
                AuthenticateMessageParams
            )
        ]
    );

    register_actor_methods!(registry, cid, [(Method::PubkeyAddress, empty)]);
}

macro_rules! register_account_version_11_onwards {
    // For versions that use types module (v15 onwards)
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

        register_actor_methods!($registry, $code_cid, [(Method::PubkeyAddress, empty)]);
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

        register_actor_methods!($registry, $code_cid, [(Method::PubkeyAddress, empty)]);
    }};
}

// register account actor methods, cid is unique for each version of the actor
pub(crate) fn register_account_actor_methods(
    registry: &mut MethodRegistry,
    cid: Cid,
    version: ActorVersion,
) {
    match version {
        ActorVersion::V8 => register_account_version_v8(registry, cid),
        ActorVersion::V9 => register_account_version_v9(registry, cid),
        ActorVersion::V10 => register_account_version_10(registry, cid),
        ActorVersion::V11 => {
            register_account_version_11_onwards!(registry, cid, fil_actor_account_state::v11)
        }
        ActorVersion::V12 => {
            register_account_version_11_onwards!(registry, cid, fil_actor_account_state::v12)
        }
        ActorVersion::V13 => {
            register_account_version_11_onwards!(registry, cid, fil_actor_account_state::v13)
        }
        ActorVersion::V14 => {
            register_account_version_11_onwards!(registry, cid, fil_actor_account_state::v14)
        }
        ActorVersion::V15 => {
            register_account_version_11_onwards!(
                registry,
                cid,
                fil_actor_account_state::v15,
                with_types
            )
        }
        ActorVersion::V16 => {
            register_account_version_11_onwards!(
                registry,
                cid,
                fil_actor_account_state::v16,
                with_types
            )
        }
        ActorVersion::V17 => {
            register_account_version_11_onwards!(
                registry,
                cid,
                fil_actor_account_state::v17,
                with_types
            )
        }
    }
}
