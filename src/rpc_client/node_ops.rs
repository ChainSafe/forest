// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_api::node_api::{NodeStatus, NODE_STATUS};

use super::{ApiInfo, JsonRpcError, RpcRequest};

impl ApiInfo {
    pub async fn node_status(&self) -> Result<NodeStatus, JsonRpcError> {
        self.call_req_e(Self::node_status_req()).await
    }

    pub fn node_status_req() -> RpcRequest<NodeStatus> {
        RpcRequest::new(NODE_STATUS, ())
    }
}
