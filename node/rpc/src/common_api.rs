// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use jsonrpc_v2::Error as JsonRpcError;
use std::convert::TryInto;

use fil_types::build_version::{user_version, APIVersion, Version, RUNNING_NODE_TYPE};
use rpc_api::common_api::*;

pub(crate) async fn version(block_delay: u64) -> Result<VersionResult, JsonRpcError> {
    let v: Version = (&*RUNNING_NODE_TYPE.read().await).try_into()?;
    Ok(APIVersion {
        version: user_version().await,
        api_version: v,
        block_delay,
    })
}
