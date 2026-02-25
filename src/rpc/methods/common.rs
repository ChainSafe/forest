// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::lotus_json::lotus_json_with_self;
use crate::rpc::error::ServerError;
use crate::rpc::{ApiPaths, Ctx, Permission, RpcMethod};
use enumflags2::BitFlags;
use fvm_ipld_blockstore::Blockstore;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::any::Any;
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

    type Params = ();
    type Ok = Uuid;

    async fn handle(
        _: Ctx<impl Any>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Uuid, ServerError> {
        Ok(*SESSION_UUID)
    }
}

pub enum Version {}
impl RpcMethod<0> for Version {
    const NAME: &'static str = "Filecoin.Version";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = PublicVersion;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(PublicVersion {
            version: crate::utils::version::FOREST_VERSION_STRING.clone(),
            // This matches Lotus's versioning for the API v1.
            // For the API v0, we don't support it but it should be `1.5.0`.
            api_version: ShiftingVersion::new(2, 3, 0),
            block_delay: ctx.chain_config().block_delay_secs,
        })
    }
}

pub enum Shutdown {}
impl RpcMethod<0> for Shutdown {
    const NAME: &'static str = "Filecoin.Shutdown";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Admin;

    type Params = ();
    type Ok = ();

    async fn handle(
        ctx: Ctx<impl Any>,
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

    type Params = ();
    type Ok = chrono::DateTime<chrono::Utc>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
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
