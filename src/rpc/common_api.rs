// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use crate::rpc_api::data_types::{APIVersion, RPCState, Version};
use fvm_ipld_blockstore::Blockstore;
use jsonrpsee::types::error::*;
use jsonrpsee::types::ErrorObjectOwned as JsonRpcError;
use once_cell::sync::Lazy;
use semver::Version as SemVer;
use tokio::sync::mpsc::Sender;

use uuid::Uuid;

static SESSION_UUID: Lazy<Uuid> = Lazy::new(Uuid::new_v4);

/// The session UUID uniquely identifies the API node.
pub(in crate::rpc) fn session() -> Result<String, JsonRpcError> {
    Ok(SESSION_UUID.to_string())
}

pub(in crate::rpc) fn version(
    block_delay: u64,
    forest_version: &'static str,
) -> Result<APIVersion, JsonRpcError> {
    let v = SemVer::parse(forest_version).unwrap();
    Ok(APIVersion {
        version: forest_version.to_string(),
        api_version: Version::new(v.major, v.minor, v.patch),
        block_delay,
    })
}

pub(in crate::rpc) async fn shutdown(shutdown_send: Sender<()>) -> Result<(), JsonRpcError> {
    // Trigger graceful shutdown
    if let Err(err) = shutdown_send.send(()).await {
        return Err(ErrorObject::owned::<i32>(
            INTERNAL_ERROR_CODE,
            err.to_string(),
            None,
        ));
    }
    Ok(())
}

/// gets start time from network
pub(in crate::rpc) fn start_time<DB: Blockstore>(
    data: &RPCState<DB>,
) -> Result<chrono::DateTime<chrono::Utc>, JsonRpcError> {
    Ok(data.start_time)
}
