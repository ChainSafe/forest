// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fil_types::build_version::{user_version, APIVersion, Version, RUNNING_NODE_TYPE};
use jsonrpc_v2::Error as JsonRpcError;
use networks::BLOCK_DELAY_SECS;
use std::convert::TryInto;

pub(crate) async fn version() -> Result<APIVersion, JsonRpcError> {
    let v: Version = (&*RUNNING_NODE_TYPE.read().await).try_into()?;
    Ok(APIVersion {
        version: user_version().await,
        api_version: v,
        block_delay: BLOCK_DELAY_SECS,
    })
}
