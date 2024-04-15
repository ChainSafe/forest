// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::sync::*;
use crate::rpc::types::RPCSyncState;
use cid::Cid;

use super::{ApiInfo, RpcRequest, ServerError};

impl ApiInfo {
    pub async fn sync_check_bad(&self, cid: Cid) -> Result<String, ServerError> {
        todo!()
    }

    pub fn sync_check_bad_req(cid: Cid) -> RpcRequest<String> {
        todo!()
    }

    pub async fn sync_mark_bad(&self, cid: Cid) -> Result<(), ServerError> {
        todo!()
    }

    pub fn sync_mark_bad_req(cid: Cid) -> RpcRequest<()> {
        todo!()
    }

    pub async fn sync_status(&self) -> Result<RPCSyncState, ServerError> {
        todo!()
    }

    pub fn sync_status_req() -> RpcRequest<RPCSyncState> {
        todo!()
    }
}
