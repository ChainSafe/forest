// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use forest_rpc_api::{
    common_api::*,
    data_types::{APIVersion, Version},
};
use jsonrpc_v2::Error as JsonRpcError;
use semver::Version as SemVer;

pub(crate) async fn version(
    block_delay: u64,
    forest_version: &'static str,
) -> Result<VersionResult, JsonRpcError> {
    let v = SemVer::parse(forest_version).unwrap();
    Ok(APIVersion {
        version: forest_version.to_string(),
        api_version: Version::new(v.major, v.minor, v.patch),
        block_delay,
    })
}
