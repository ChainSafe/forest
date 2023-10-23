// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_api::node_api::{NodeStatus, NodeStatusParams, NodeStatusResult, NODE_STATUS};
use jsonrpc_v2::Error;

use crate::rpc_client::call;

use super::RpcRequest;

pub async fn node_status(
    (): NodeStatusParams,
    auth_token: &Option<String>,
) -> Result<NodeStatusResult, Error> {
    call(NODE_STATUS, (), auth_token).await
}

pub fn node_status_req() -> RpcRequest<NodeStatus> {
    RpcRequest::new(NODE_STATUS, ())
}
