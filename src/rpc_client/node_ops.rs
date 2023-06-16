// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_api::node_api::{NodeStatusParams, NodeStatusResult, NODE_STATUS};
use jsonrpc_v2::Error;

use crate::rpc_client::call;

pub async fn node_status(
    params: NodeStatusParams,
    auth_token: &Option<String>,
) -> Result<NodeStatusResult, Error> {
    call(NODE_STATUS, params, auth_token).await
}
