// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::node_api::{NodeStatus, NODE_STATUS};

use super::{ApiInfo, RpcRequest, ServerError};

impl ApiInfo {
    pub async fn node_status(&self) -> Result<NodeStatus, ServerError> {
        self.call(Self::node_status_req()).await
    }

    pub fn node_status_req() -> RpcRequest<NodeStatus> {
        RpcRequest::new(NODE_STATUS, ())
    }
}
