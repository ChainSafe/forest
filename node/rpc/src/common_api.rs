// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use jsonrpc_v2::Error as JsonRpcError;

use fil_types::build_version::{APIVersion, Version};
use rpc_api::common_api::*;

pub(crate) async fn version(
    block_delay: u64,
    forest_version: &'static str,
) -> Result<VersionResult, JsonRpcError> {
    Ok(APIVersion {
        version: forest_version.to_string(),
        api_version: Version::new(0, 0, 0),
        block_delay,
    })
}
