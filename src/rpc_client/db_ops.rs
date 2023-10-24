// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_api::db_api::*;

use super::{ApiInfo, JsonRpcError, RpcRequest};

impl ApiInfo {
    pub async fn db_gc(&self) -> Result<(), JsonRpcError> {
        self.call_req_e(Self::db_gc_req()).await
    }

    pub fn db_gc_req() -> RpcRequest<()> {
        RpcRequest::new(DB_GC, ())
    }
}
