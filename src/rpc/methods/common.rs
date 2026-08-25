// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::lotus_json::lotus_json_with_self;
use crate::rpc::error::ServerError;
use crate::rpc::{ApiPaths, Ctx, Permission, RpcMethod};
use enumflags2::BitFlags;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use uuid::Uuid;

static SESSION_UUID: LazyLock<Uuid> = LazyLock::new(crate::utils::rand::new_uuid_v4);

/// The returned session UUID uniquely identifies the API node.
pub enum Session {}
impl RpcMethod<0> for Session {
    const NAME: &'static str = "Filecoin.Session";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: &'static str =
        "Returns a UUID that uniquely identifies this node for the current session.";

    type Params = ();
    type Ok = Uuid;

    async fn handle(_: Ctx, (): Self::Params, _: &http::Extensions) -> Result<Uuid, ServerError> {
        Ok(*SESSION_UUID)
    }
}

pub enum Version {}
impl RpcMethod<0> for Version {
    const NAME: &'static str = "Filecoin.Version";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: &'static str = "Returns the node version, API version, and block delay.";

    type Params = ();
    type Ok = PublicVersion;

    async fn handle(
        ctx: Ctx,
        (): Self::Params,
        ext: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        // Report the API version for the endpoint actually being served, so V0
        // clients (e.g. lotus-miner over `/rpc/v0`) accept the version handshake.
        // Values from Lotus `api/version.go`: <https://github.com/filecoin-project/lotus/blob/27abf0f16a7f2a83305910f3c2a1844764d20b75/api/version.go#L57-L58>
        let api_version = match ext.get::<ApiPaths>() {
            Some(ApiPaths::V0) => ShiftingVersion::new(1, 5, 0),
            Some(ApiPaths::V1 | ApiPaths::V2) | None => ShiftingVersion::new(2, 3, 0),
        };
        Ok(PublicVersion {
            version: crate::utils::version::FOREST_VERSION_STRING.clone(),
            api_version,
            block_delay: ctx.chain_config().block_delay_secs,
            agent: "forest".into(),
        })
    }
}

pub enum Shutdown {}
impl RpcMethod<0> for Shutdown {
    const NAME: &'static str = "Filecoin.Shutdown";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Admin;
    const DESCRIPTION: &'static str = "Shuts the node down.";

    type Params = ();
    type Ok = ();

    async fn handle(
        ctx: Ctx,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        ctx.shutdown.send(()).await?;
        Ok(())
    }
}

pub enum StartTime {}
impl RpcMethod<0> for StartTime {
    const NAME: &'static str = "Filecoin.StartTime";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: &'static str = "Returns the time at which the node was started.";

    type Params = ();
    type Ok = chrono::DateTime<chrono::Utc>;

    async fn handle(
        ctx: Ctx,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(ctx.start_time)
    }
}

/// Represents the current version of the API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct PublicVersion {
    pub version: String,
    #[serde(rename = "APIVersion")]
    pub api_version: ShiftingVersion,
    pub block_delay: u32,
    // See <https://github.com/filecoin-project/lotus/blob/a0ecb8687f1c60d5e66040b6de364dbc9cc4d253/api/api_common.go#L78>
    pub agent: String,
}
lotus_json_with_self!(PublicVersion);

/// Integer based value on version information. Highest order bits for Major,
/// Mid order for Minor and lowest for Patch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ShiftingVersion(u32);

impl ShiftingVersion {
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self(((major as u32) << 16) | ((minor as u32) << 8) | (patch as u32))
    }
}
