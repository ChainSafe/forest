// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use jsonrpc_v2::Error as JsonRpcError;
use std::convert::TryInto;

use fil_types::build_version::{user_version, APIVersion, Version, RUNNING_NODE_TYPE};
use networks::BLOCK_DELAY_SECS;
use rpc_api::common_api::*;

pub(crate) async fn version() -> Result<VersionResult, JsonRpcError> {
    let v: Version = (&*RUNNING_NODE_TYPE.read().await).try_into()?;
    Ok(APIVersion {
        version: user_version().await,
        api_version: v,
        block_delay: BLOCK_DELAY_SECS,
    })
}
