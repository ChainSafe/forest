// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::registry::methods_reg::{MethodRegistry, register_actor_methods};
use crate::shim::message::MethodNum;
use cid::Cid;
use fil_actors_shared::actor_versions::ActorVersion;

// Macro for versions 8-9 that have limited methods
macro_rules! register_power_versions_8_to_9 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            CreateMinerParams, EnrollCronEventParams, Method, UpdateClaimedPowerParams,
        };

        // Register methods with parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::CreateMiner, CreateMinerParams),
                (Method::UpdateClaimedPower, UpdateClaimedPowerParams),
                (Method::EnrollCronEvent, EnrollCronEventParams),
            ]
        );

        // Register methods without parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, empty),
                (Method::OnEpochTickEnd, empty),
                (Method::CurrentTotalPower, empty),
            ]
        );
    }};
}

// Macro for versions 10-15 that have most methods but not MinerPowerParams
macro_rules! register_power_versions_10_to_15 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            CreateMinerParams, EnrollCronEventParams, Method, MinerRawPowerParams,
            UpdateClaimedPowerParams, UpdatePledgeTotalParams,
        };

        // Register methods with parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::CreateMiner, CreateMinerParams),
                (Method::UpdateClaimedPower, UpdateClaimedPowerParams),
                (Method::EnrollCronEvent, EnrollCronEventParams),
                (Method::UpdatePledgeTotal, UpdatePledgeTotalParams),
                (Method::CreateMinerExported, CreateMinerParams),
                (Method::MinerRawPowerExported, MinerRawPowerParams),
            ]
        );

        // Register methods without parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, empty),
                (Method::OnEpochTickEnd, empty),
                (Method::CurrentTotalPower, empty),
                (Method::NetworkRawPowerExported, empty),
                (Method::MinerCountExported, empty),
                (Method::MinerConsensusCountExported, empty),
            ]
        );
    }};
}

// Macro for version 16 that has all methods
macro_rules! register_power_version_16 {
    ($registry:expr, $code_cid:expr, $state_version:path) => {{
        use $state_version::{
            CreateMinerParams, EnrollCronEventParams, Method, MinerPowerParams,
            MinerRawPowerParams, UpdateClaimedPowerParams, UpdatePledgeTotalParams,
        };

        // Register methods with parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::CreateMiner, CreateMinerParams),
                (Method::UpdateClaimedPower, UpdateClaimedPowerParams),
                (Method::EnrollCronEvent, EnrollCronEventParams),
                (Method::UpdatePledgeTotal, UpdatePledgeTotalParams),
                (Method::CreateMinerExported, CreateMinerParams),
                (Method::MinerRawPowerExported, MinerRawPowerParams),
                (Method::MinerPowerExported, MinerPowerParams),
            ]
        );

        // Register methods without parameters
        register_actor_methods!(
            $registry,
            $code_cid,
            [
                (Method::Constructor, empty),
                (Method::OnEpochTickEnd, empty),
                (Method::CurrentTotalPower, empty),
                (Method::NetworkRawPowerExported, empty),
                (Method::MinerCountExported, empty),
                (Method::MinerConsensusCountExported, empty),
            ]
        );
    }};
}

pub(crate) fn register_actor_methods(
    registry: &mut MethodRegistry,
    cid: Cid,
    version: ActorVersion,
) {
    match version {
        ActorVersion::V8 => {
            register_power_versions_8_to_9!(registry, cid, fil_actor_power_state::v8)
        }
        ActorVersion::V9 => {
            register_power_versions_8_to_9!(registry, cid, fil_actor_power_state::v9)
        }
        ActorVersion::V10 => {
            register_power_versions_10_to_15!(registry, cid, fil_actor_power_state::v10)
        }
        ActorVersion::V11 => {
            register_power_versions_10_to_15!(registry, cid, fil_actor_power_state::v11)
        }
        ActorVersion::V12 => {
            register_power_versions_10_to_15!(registry, cid, fil_actor_power_state::v12)
        }
        ActorVersion::V13 => {
            register_power_versions_10_to_15!(registry, cid, fil_actor_power_state::v13)
        }
        ActorVersion::V14 => {
            register_power_versions_10_to_15!(registry, cid, fil_actor_power_state::v14)
        }
        ActorVersion::V15 => {
            register_power_versions_10_to_15!(registry, cid, fil_actor_power_state::v15)
        }
        ActorVersion::V16 => register_power_version_16!(registry, cid, fil_actor_power_state::v16),
    }
}
