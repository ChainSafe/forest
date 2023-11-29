// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc_api::progress_api::*;

use super::{ApiInfo, JsonRpcError, RpcRequest};

impl ApiInfo {
    pub async fn get_progress(
        &self,
        progress_type: GetProgressType,
    ) -> Result<GetProgressResult, JsonRpcError> {
        self.call(Self::get_progress_req(progress_type)).await
    }

    pub fn get_progress_req(progress_type: GetProgressType) -> RpcRequest<(u64, u64)> {
        RpcRequest::new(GET_PROGRESS, progress_type)
    }
}
