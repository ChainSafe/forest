// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::address::Address;
use crate::shim::message::MethodNum;
use cid::Cid;
use fil_actors_shared::actor_versions::ActorVersion;
use paste::paste;

// Core methods present in all versions
macro_rules! register_core_methods {
    ($registry:expr, $cid:expr, v8) => {{
        use fil_actor_verifreg_state::v8::{Method, RemoveDataCapParams, VerifierParams};
        register_actor_methods!(
            $registry,
            $cid,
            [
                (Method::Constructor, Address),
                (Method::AddVerifier, VerifierParams),
                (Method::RemoveVerifier, Address),
                (Method::AddVerifiedClient, VerifierParams),
                (Method::RemoveVerifiedClientDataCap, RemoveDataCapParams),
            ]
        );
    }};

    ($registry:expr, $cid:expr, v9) => {{
        use fil_actor_verifreg_state::v9::{
            AddVerifierClientParams, AddVerifierParams, Method, RemoveDataCapParams,
        };
        register_actor_methods!(
            $registry,
            $cid,
            [
                (Method::Constructor, Address),
                (Method::AddVerifier, AddVerifierParams),
                (Method::RemoveVerifier, Address),
                (Method::AddVerifiedClient, AddVerifierClientParams),
                (Method::RemoveVerifiedClientDataCap, RemoveDataCapParams),
            ]
        );
    }};

    ($registry:expr, $cid:expr, v10) => {{
        use fil_actor_verifreg_state::v10::{
            AddVerifiedClientParams, AddVerifierParams, Method, RemoveDataCapParams,
        };
        register_actor_methods!(
            $registry,
            $cid,
            [
                (Method::Constructor, Address),
                (Method::AddVerifier, AddVerifierParams),
                (Method::RemoveVerifier, Address),
                (Method::AddVerifiedClient, AddVerifiedClientParams),
                (Method::RemoveVerifiedClientDataCap, RemoveDataCapParams),
            ]
        );
    }};

    ($registry:expr, $cid:expr, $state_version:path) => {{
        use $state_version::{
            AddVerifiedClientParams, AddVerifierParams, ConstructorParams, Method,
            RemoveDataCapParams, RemoveVerifierParams,
        };
        register_actor_methods!(
            $registry,
            $cid,
            [
                (Method::Constructor, ConstructorParams),
                (Method::AddVerifier, AddVerifierParams),
                (Method::RemoveVerifier, RemoveVerifierParams),
                (Method::AddVerifiedClient, AddVerifiedClientParams),
                (Method::RemoveVerifiedClientDataCap, RemoveDataCapParams),
            ]
        );
    }};
}

// unique methods (UseBytes/RestoreBytes)
macro_rules! register_v8_unique_methods {
    ($registry:expr, $cid:expr) => {{
        use fil_actor_verifreg_state::v8::{BytesParams, Method};
        register_actor_methods!(
            $registry,
            $cid,
            [
                (Method::UseBytes, BytesParams),
                (Method::RestoreBytes, BytesParams),
            ]
        );
    }};
}

// Allocation/Claims methods
macro_rules! register_allocation_methods {
    ($registry:expr, $cid:expr, v9) => {{
        use fil_actor_verifreg_state::v9::{
            ClaimAllocationsParams, ExtendClaimTermsParams, GetClaimsParams, Method,
            RemoveExpiredAllocationsParams, RemoveExpiredClaimsParams,
        };
        register_actor_methods!(
            $registry,
            $cid,
            [
                (
                    Method::RemoveExpiredAllocations,
                    RemoveExpiredAllocationsParams
                ),
                (Method::ClaimAllocations, ClaimAllocationsParams),
                (Method::GetClaims, GetClaimsParams),
                (Method::ExtendClaimTerms, ExtendClaimTermsParams),
                (Method::RemoveExpiredClaims, RemoveExpiredClaimsParams),
            ]
        );
    }};

    ($registry:expr, $cid:expr, $state_version:path, no_claim) => {{
        use $state_version::{
            ExtendClaimTermsParams, GetClaimsParams, Method, RemoveExpiredAllocationsParams,
            RemoveExpiredClaimsParams,
        };
        register_actor_methods!(
            $registry,
            $cid,
            [
                (
                    Method::RemoveExpiredAllocations,
                    RemoveExpiredAllocationsParams
                ),
                (Method::GetClaims, GetClaimsParams),
                (Method::ExtendClaimTerms, ExtendClaimTermsParams),
                (Method::RemoveExpiredClaims, RemoveExpiredClaimsParams),
            ]
        );
    }};

    ($registry:expr, $cid:expr, $state_version:path) => {{
        use $state_version::{
            ClaimAllocationsParams, ExtendClaimTermsParams, GetClaimsParams, Method,
            RemoveExpiredAllocationsParams, RemoveExpiredClaimsParams,
        };
        register_actor_methods!(
            $registry,
            $cid,
            [
                (
                    Method::RemoveExpiredAllocations,
                    RemoveExpiredAllocationsParams
                ),
                (Method::ClaimAllocations, ClaimAllocationsParams),
                (Method::GetClaims, GetClaimsParams),
                (Method::ExtendClaimTerms, ExtendClaimTermsParams),
                (Method::RemoveExpiredClaims, RemoveExpiredClaimsParams),
            ]
        );
    }};
}

macro_rules! register_exported_methods {
    ($registry:expr, $cid:expr, $state_version:path) => {{
        use $state_version::{
            AddVerifiedClientParams, ExtendClaimTermsParams, GetClaimsParams, Method,
            RemoveExpiredAllocationsParams, RemoveExpiredClaimsParams,
        };
        register_actor_methods!(
            $registry,
            $cid,
            [
                (Method::AddVerifiedClientExported, AddVerifiedClientParams),
                (
                    Method::RemoveExpiredAllocationsExported,
                    RemoveExpiredAllocationsParams
                ),
                (Method::GetClaimsExported, GetClaimsParams),
                (Method::ExtendClaimTermsExported, ExtendClaimTermsParams),
                (
                    Method::RemoveExpiredClaimsExported,
                    RemoveExpiredClaimsParams
                ),
            ]
        );
    }};
}

macro_rules! register_universal_receiver_hook {
    ($registry:expr, $cid:expr, $version:tt) => {
        paste! {
            {
                use fil_actor_verifreg_state::[<$version>]::Method;
                register_actor_methods!(
                    $registry,
                    $cid,
                    [(
                        Method::UniversalReceiverHook,
                        fvm_actor_utils::receiver::UniversalReceiverParams
                    ),]
                );
            }
        }
    };
}

fn register_verified_reg_v8(registry: &mut MethodRegistry, cid: Cid) {
    register_core_methods!(registry, cid, v8);
    register_v8_unique_methods!(registry, cid);
}

fn register_verified_reg_v9(registry: &mut MethodRegistry, cid: Cid) {
    register_core_methods!(registry, cid, v9);
    register_allocation_methods!(registry, cid, v9);
    register_universal_receiver_hook!(registry, cid, v9);
}

fn register_verified_reg_v10(registry: &mut MethodRegistry, cid: Cid) {
    register_core_methods!(registry, cid, v10);
    register_allocation_methods!(registry, cid, fil_actor_verifreg_state::v10, no_claim);
    register_exported_methods!(registry, cid, fil_actor_verifreg_state::v10);
    register_universal_receiver_hook!(registry, cid, v10);
}

fn register_verified_reg_v11(registry: &mut MethodRegistry, cid: Cid) {
    register_core_methods!(registry, cid, fil_actor_verifreg_state::v11);
    register_allocation_methods!(registry, cid, fil_actor_verifreg_state::v11, no_claim);
    register_exported_methods!(registry, cid, fil_actor_verifreg_state::v11);
    register_universal_receiver_hook!(registry, cid, v11);
}

macro_rules! register_verified_reg_v12_plus {
    ($registry:expr, $cid:expr, $state_version:path, $version:tt) => {{
        register_core_methods!($registry, $cid, $state_version);
        register_allocation_methods!($registry, $cid, $state_version);
        register_exported_methods!($registry, $cid, $state_version);
        register_universal_receiver_hook!($registry, $cid, $version);
    }};
}

pub(crate) fn register_actor_methods(
    registry: &mut MethodRegistry,
    cid: Cid,
    version: ActorVersion,
) {
    match version {
        ActorVersion::V8 => register_verified_reg_v8(registry, cid),
        ActorVersion::V9 => register_verified_reg_v9(registry, cid),
        ActorVersion::V10 => register_verified_reg_v10(registry, cid),
        ActorVersion::V11 => register_verified_reg_v11(registry, cid),
        ActorVersion::V12 => {
            register_verified_reg_v12_plus!(registry, cid, fil_actor_verifreg_state::v12, v12)
        }
        ActorVersion::V13 => {
            register_verified_reg_v12_plus!(registry, cid, fil_actor_verifreg_state::v13, v13)
        }
        ActorVersion::V14 => {
            register_verified_reg_v12_plus!(registry, cid, fil_actor_verifreg_state::v14, v14)
        }
        ActorVersion::V15 => {
            register_verified_reg_v12_plus!(registry, cid, fil_actor_verifreg_state::v15, v15)
        }
        ActorVersion::V16 => {
            register_verified_reg_v12_plus!(registry, cid, fil_actor_verifreg_state::v16, v16)
        }
    }
}
