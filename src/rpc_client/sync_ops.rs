// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::sync_api::*;
use crate::rpc::types::RPCSyncState;
use cid::Cid;

use super::{ApiInfo, JsonRpcError, RpcRequest};

impl ApiInfo {
    pub async fn sync_check_bad(&self, cid: Cid) -> Result<String, JsonRpcError> {
        self.call(Self::sync_check_bad_req(cid)).await
    }

    pub fn sync_check_bad_req(cid: Cid) -> RpcRequest<String> {
        RpcRequest::new(SYNC_CHECK_BAD, (cid,))
    }

    pub async fn sync_mark_bad(&self, cid: Cid) -> Result<(), JsonRpcError> {
        self.call(Self::sync_mark_bad_req(cid)).await
    }

    pub fn sync_mark_bad_req(cid: Cid) -> RpcRequest<()> {
        RpcRequest::new(SYNC_MARK_BAD, (cid,))
    }

    pub async fn sync_status(&self) -> Result<RPCSyncState, JsonRpcError> {
        self.call(Self::sync_status_req()).await
    }

    pub fn sync_status_req() -> RpcRequest<RPCSyncState> {
        RpcRequest::new(SYNC_STATE, ())
    }
}
